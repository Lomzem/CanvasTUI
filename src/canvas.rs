use color_eyre::eyre::{Context, Result, bail};
use reqwest::{Client, Url, header};
use serde::Deserialize;
use serde_json::Value;
use time::OffsetDateTime;

use crate::domain::{
    AgendaDay, AgendaItem, AgendaSnapshot, DateRange, ItemKind, compare_items, local_offset,
};

#[derive(Clone)]
pub struct CanvasClient {
    client: Client,
    base_url: Url,
    access_token: String,
}

impl CanvasClient {
    pub fn new(base_url: Url, access_token: String) -> Self {
        Self {
            client: Client::new(),
            base_url,
            access_token,
        }
    }

    pub async fn fetch_snapshot(&self, range: DateRange) -> Result<AgendaSnapshot> {
        let local_offset = local_offset();
        let mut items = self.fetch_planner_items(range).await?;
        items.sort_by(compare_items);

        let mut days: Vec<AgendaDay> = Vec::new();
        for item in items {
            let item_date = item.local_date(local_offset);
            match days.last_mut() {
                Some(day) if day.date == item_date => day.items.push(item),
                _ => days.push(AgendaDay {
                    date: item_date,
                    items: vec![item],
                }),
            }
        }

        Ok(AgendaSnapshot {
            loaded_range: range,
            fetched_at: OffsetDateTime::now_utc(),
            days,
        })
    }

    async fn fetch_planner_items(&self, range: DateRange) -> Result<Vec<AgendaItem>> {
        let mut next_url = Some(self.planner_items_url(range)?);
        let mut items = Vec::new();

        while let Some(url) = next_url.take() {
            let response = self
                .client
                .get(url.clone())
                .bearer_auth(&self.access_token)
                .send()
                .await
                .wrap_err_with(|| format!("failed to request {}", url))?
                .error_for_status()
                .wrap_err("Canvas returned an error response")?;

            let link_header = response
                .headers()
                .get(header::LINK)
                .and_then(|value| value.to_str().ok())
                .map(str::to_owned);
            let body = response
                .bytes()
                .await
                .wrap_err("failed to read Canvas response body")?;
            let page: Vec<PlannerItemDto> =
                serde_json::from_slice(&body).wrap_err("failed to parse planner response")?;
            for item in page {
                items.push(item.normalize(&self.base_url)?);
            }

            next_url = link_header
                .as_deref()
                .and_then(parse_next_link)
                .map(|raw| raw.parse::<Url>())
                .transpose()
                .wrap_err("failed to parse Canvas pagination link")?;
        }

        Ok(items)
    }

    fn planner_items_url(&self, range: DateRange) -> Result<Url> {
        let mut url = self.base_url.join("/api/v1/planner/items")?;
        url.query_pairs_mut()
            .append_pair("start_date", &range.start.to_string())
            .append_pair("end_date", &range.end.to_string())
            .append_pair("per_page", "100");
        Ok(url)
    }
}

#[derive(Debug, Deserialize)]
struct PlannerItemDto {
    #[serde(deserialize_with = "de_string")]
    plannable_id: String,
    plannable_type: String,
    #[serde(default, deserialize_with = "de_opt_datetime")]
    plannable_date: Option<OffsetDateTime>,
    #[serde(default)]
    html_url: Option<String>,
    #[serde(default)]
    context_name: Option<String>,
    #[serde(default)]
    planner_override: Option<PlannerOverrideDto>,
    #[serde(default)]
    submissions: SubmissionField,
    #[serde(default)]
    plannable: PlannableDto,
    #[serde(default)]
    new_activity: bool,
}

impl PlannerItemDto {
    fn normalize(self, base_url: &Url) -> Result<AgendaItem> {
        let occurs_at = self
            .plannable_date
            .or_else(|| self.plannable.best_date())
            .ok_or_else(|| {
                color_eyre::eyre::eyre!(
                    "planner item {} is missing plannable_date",
                    self.plannable_id
                )
            })?;
        let title = self
            .plannable
            .title
            .clone()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| fallback_title(&self.plannable_type, &self.plannable_id));
        let context_name = self
            .context_name
            .clone()
            .or_else(|| self.plannable.course_label())
            .unwrap_or_else(|| "Personal".to_string());
        let submission = self.submissions.submission();
        let submitted = submission
            .and_then(|state| state.submitted)
            .unwrap_or(false);
        let completed = self
            .planner_override
            .as_ref()
            .and_then(|override_state| override_state.marked_complete)
            .unwrap_or(false)
            || submitted;

        Ok(AgendaItem {
            key: format!("{}:{}", self.plannable_type, self.plannable_id),
            id: self.plannable_id,
            kind: ItemKind(self.plannable_type),
            title,
            context_name,
            occurs_at: occurs_at.to_offset(local_offset()),
            html_url: self
                .html_url
                .as_deref()
                .map(|url| normalize_html_url(base_url, url))
                .transpose()?,
            completed,
            submitted,
            missing: submission.and_then(|state| state.missing).unwrap_or(false),
            late: submission.and_then(|state| state.late).unwrap_or(false),
            needs_grading: submission
                .and_then(|state| state.needs_grading)
                .unwrap_or(false),
            new_activity: self.new_activity,
            details: self.plannable.details.clone(),
        })
    }
}

#[derive(Debug, Default, Deserialize)]
struct PlannableDto {
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    details: Option<String>,
    #[serde(default, deserialize_with = "de_opt_datetime")]
    todo_date: Option<OffsetDateTime>,
    #[serde(default, deserialize_with = "de_opt_datetime")]
    due_at: Option<OffsetDateTime>,
    #[serde(default, deserialize_with = "de_opt_datetime")]
    start_at: Option<OffsetDateTime>,
    #[serde(default, deserialize_with = "de_opt_datetime")]
    created_at: Option<OffsetDateTime>,
    #[serde(default)]
    location_name: Option<String>,
    #[serde(default)]
    course_id: Option<Value>,
}

impl PlannableDto {
    fn best_date(&self) -> Option<OffsetDateTime> {
        self.todo_date
            .or(self.due_at)
            .or(self.start_at)
            .or(self.created_at)
    }

    fn course_label(&self) -> Option<String> {
        self.location_name
            .clone()
            .or_else(|| self.course_id.as_ref().map(|_| "Course".to_string()))
    }
}

#[derive(Debug, Deserialize)]
struct PlannerOverrideDto {
    #[serde(default)]
    marked_complete: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum SubmissionField {
    Bool(#[allow(dead_code)] bool),
    Object(SubmissionState),
}

impl Default for SubmissionField {
    fn default() -> Self {
        Self::Bool(false)
    }
}

impl SubmissionField {
    fn submission(&self) -> Option<&SubmissionState> {
        match self {
            Self::Object(state) => Some(state),
            Self::Bool(_) => None,
        }
    }
}

#[derive(Debug, Deserialize)]
struct SubmissionState {
    #[serde(default)]
    submitted: Option<bool>,
    #[serde(default)]
    late: Option<bool>,
    #[serde(default)]
    missing: Option<bool>,
    #[serde(default)]
    needs_grading: Option<bool>,
}

fn parse_next_link(link_header: &str) -> Option<&str> {
    link_header.split(',').find_map(|entry| {
        let mut parts = entry.split(';');
        let url = parts.next()?.trim();
        let rel = parts.find(|part| part.trim() == "rel=\"next\"")?;
        let _ = rel;
        url.strip_prefix('<')?.strip_suffix('>')
    })
}

fn normalize_html_url(base_url: &Url, raw_url: &str) -> Result<String> {
    if raw_url.trim().is_empty() {
        bail!("empty html_url");
    }
    if let Ok(url) = raw_url.parse::<Url>() {
        return Ok(url.to_string());
    }
    Ok(base_url.join(raw_url)?.to_string())
}

fn fallback_title(kind: &str, id: &str) -> String {
    match kind {
        "planner_note" => format!("Note {id}"),
        "calendar_event" => format!("Event {id}"),
        _ => format!("{} {id}", kind.replace('_', " ")),
    }
}

fn de_string<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = Value::deserialize(deserializer)?;
    match value {
        Value::String(value) => Ok(value),
        Value::Number(value) => Ok(value.to_string()),
        _ => Err(serde::de::Error::custom("expected string or number")),
    }
}

fn de_opt_datetime<'de, D>(deserializer: D) -> Result<Option<OffsetDateTime>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = Option::<String>::deserialize(deserializer)?;
    value
        .map(|value| OffsetDateTime::parse(&value, &time::format_description::well_known::Rfc3339))
        .transpose()
        .map_err(serde::de::Error::custom)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_next_link() {
        let link = r#"<https://canvas.example/api/v1/planner/items?page=2>; rel="next", <https://canvas.example/api/v1/planner/items?page=10>; rel="last""#;
        assert_eq!(
            parse_next_link(link),
            Some("https://canvas.example/api/v1/planner/items?page=2")
        );
    }

    #[test]
    fn normalizes_relative_url() {
        let base = Url::parse("https://canvas.example").unwrap();
        let normalized = normalize_html_url(&base, "/courses/1/assignments/2").unwrap();
        assert_eq!(normalized, "https://canvas.example/courses/1/assignments/2");
    }
}
