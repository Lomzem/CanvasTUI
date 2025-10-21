use std::{collections::BTreeMap, env};

use color_eyre::eyre::Result;
use ratatui::widgets::TableState;
use reqwest::Url;
use serde::{Deserialize, de::Visitor};
use time::{Date, OffsetDateTime, PrimitiveDateTime, UtcOffset, format_description};
use tokio::sync::mpsc::UnboundedSender;

use crate::{Action, CACHE_FILE};

const ENDPOINT: &str = "/api/v1/planner/items";

#[derive(Debug, Clone)]
pub struct Calendar {
    pub dates: Vec<CalendarDate>,
}

#[derive(Debug, Clone)]
pub struct CalendarDate {
    pub events: Vec<CalendarEvent>,
    pub table_state: TableState,
}

#[derive(Debug, Clone)]
pub struct CalendarEvent {
    pub course_name: String,
    pub due_at: PrimitiveDateTime,
    pub title: String,
    pub html_url: String,
    pub submitted: bool,
}

#[derive(Debug, Deserialize)]
struct CanvasPlannerNote {
    context_name: String,
    html_url: String,
    submissions: SubmissionStatus,
    plannable: CanvasPlannable,
    #[serde(deserialize_with = "time::serde::iso8601::deserialize")]
    plannable_date: OffsetDateTime,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum SubmissionStatus {
    Bool(bool),
    Object { submitted: bool },
}

#[derive(Debug, Deserialize)]
struct CanvasPlannable {
    title: String,
}

struct CalendarVisitor {}

impl<'de> Visitor<'de> for CalendarVisitor {
    type Value = Calendar;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("a sequence of CanvasPlannerNote")
    }
    fn visit_seq<A>(self, mut seq: A) -> std::result::Result<Self::Value, A::Error>
    where
        A: serde::de::SeqAccess<'de>,
    {
        let mut events: BTreeMap<Date, Vec<CalendarEvent>> = BTreeMap::new();

        while let Some(item) = seq.next_element::<CanvasPlannerNote>()? {
            let local_due_at = item.plannable_date;
            let submission_status = match item.submissions {
                SubmissionStatus::Bool(submitted) => submitted,
                SubmissionStatus::Object { submitted } => submitted,
            };
            events
                .entry(local_due_at.date())
                .or_default()
                .push(CalendarEvent {
                    course_name: item
                        .context_name
                        .split_whitespace()
                        .take(2)
                        .collect::<Vec<&str>>()
                        .join("-"),
                    due_at: {
                        /* Remove timezone info */
                        let local_offset = UtcOffset::current_local_offset().unwrap();
                        let local_odt = item.plannable_date.to_offset(local_offset);
                        PrimitiveDateTime::new(local_odt.date(), local_odt.time())
                    },
                    title: item.plannable.title,
                    html_url: item.html_url,
                    submitted: submission_status,
                });
        }

        let dates: Vec<_> = events
            .into_iter()
            .map(|(_, events)| CalendarDate {
                events: events,
                table_state: TableState::default().with_selected(0),
            })
            .collect();

        Ok(Calendar { dates })
    }
}

impl<'de> Deserialize<'de> for Calendar {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_seq(CalendarVisitor {})
    }
}

pub async fn fetch(action_tx: &mut UnboundedSender<Action>) -> Result<()> {
    let access_token = env::var("CANVAS_ACCESS_TOKEN")?;

    let mut url = env::var("CANVAS_URL")
        .unwrap()
        .parse::<Url>()
        .unwrap()
        .join(ENDPOINT)?;

    let current_date = OffsetDateTime::now_local()
        .expect("Could not get current date")
        .date();
    url.query_pairs_mut()
        .append_pair("access_token", &access_token)
        .append_pair(
            "start_date",
            &current_date
                .format(&format_description::parse("[year]-[month]-[day]").unwrap())
                .expect("Could not format date")
                .to_string(),
        );

    let response = reqwest::get(url).await?;
    let body_bytes = response.bytes().await?;
    let calendar: Calendar = serde_json::from_slice(&body_bytes)?;
    action_tx.send(Action::FetchComplete(calendar))?;
    tokio::fs::write(CACHE_FILE, &body_bytes).await?;
    Ok(())
}
