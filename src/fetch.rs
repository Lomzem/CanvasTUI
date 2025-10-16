use std::{collections::BTreeMap, env};

use chrono::{DateTime, Local, NaiveDate};
use color_eyre::eyre::Result;
use ratatui::widgets::TableState;
use reqwest::Url;
use serde::{Deserialize, de::Visitor};
use tokio::sync::mpsc::UnboundedSender;

use crate::{Action, CACHE_FILE};

const ENDPOINT: &str = "/api/v1/planner/items";

#[derive(Debug, Clone)]
pub struct Calendar {
    pub dates: Vec<CalendarDate>,
    pub current_date_index: usize,
}

#[derive(Debug, Clone)]
pub struct CalendarDate {
    pub events: Vec<CalendarEvent>,
    pub table_state: TableState,
}

#[derive(Debug, Clone)]
pub struct CalendarEvent {
    pub course_name: String,
    pub due_at: DateTime<Local>,
    pub title: String,
    pub html_url: String,
    pub submitted: bool,
}

#[derive(Debug, Deserialize)]
struct CanvasPlannerNote {
    context_name: String,
    html_url: String,
    submissions: CanvasSubmissions,
    plannable: CanvasPlannable,
}

#[derive(Debug, Deserialize)]
struct CanvasSubmissions {
    submitted: bool,
}

#[derive(Debug, Deserialize)]
struct CanvasPlannable {
    title: String,
    due_at: DateTime<Local>,
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
        let mut events: BTreeMap<NaiveDate, Vec<CalendarEvent>> = BTreeMap::new();

        while let Some(item) = seq.next_element::<CanvasPlannerNote>()? {
            let local_due_at = item.plannable.due_at.with_timezone(&Local);
            events
                .entry(local_due_at.date_naive())
                .or_default()
                .push(CalendarEvent {
                    course_name: item
                        .context_name
                        .split_whitespace()
                        .take(2)
                        .collect::<Vec<&str>>()
                        .join("-"),
                    due_at: item.plannable.due_at.with_timezone(&Local),
                    title: item.plannable.title,
                    html_url: item.html_url,
                    submitted: item.submissions.submitted,
                });
        }

        let dates: Vec<_> = events
            .into_iter()
            .map(|(_, events)| CalendarDate {
                events: events,
                table_state: TableState::default().with_selected(0),
            })
            .collect();

        Ok(Calendar {
            dates,
            current_date_index: 0,
        })
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

    url.query_pairs_mut()
        .append_pair("access_token", &access_token)
        .append_pair("start_date", &Local::now().format("%Y-%m-%d").to_string());

    let response = reqwest::get(url).await?;
    let body_bytes = response.bytes().await?;
    let calendar: Calendar = serde_json::from_slice(&body_bytes)?;
    action_tx.send(Action::FetchComplete(calendar))?;
    tokio::fs::write(CACHE_FILE, &body_bytes).await?;
    Ok(())
}
