use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct DateRange {
    pub start: time::Date,
    pub end: time::Date,
}

impl DateRange {
    pub fn new(start: time::Date, end: time::Date) -> Self {
        Self { start, end }
    }

    pub fn merge(self, other: Self) -> Self {
        Self {
            start: self.start.min(other.start),
            end: self.end.max(other.end),
        }
    }

    pub fn days_before(&self, days: i64) -> Self {
        Self {
            start: self.start - time::Duration::days(days),
            end: self.start - time::Duration::days(1),
        }
    }

    pub fn days_after(&self, days: i64) -> Self {
        Self {
            start: self.end + time::Duration::days(1),
            end: self.end + time::Duration::days(days),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AgendaSnapshot {
    pub loaded_range: DateRange,
    #[serde(with = "time::serde::iso8601")]
    pub fetched_at: OffsetDateTime,
    pub days: Vec<AgendaDay>,
}

impl AgendaSnapshot {
    pub fn empty(loaded_range: DateRange, fetched_at: OffsetDateTime) -> Self {
        Self {
            loaded_range,
            fetched_at,
            days: Vec::new(),
        }
    }

    pub fn merge(&self, other: Self) -> Self {
        let mut items: BTreeMap<String, AgendaItem> = BTreeMap::new();
        for day in self.days.iter().chain(other.days.iter()) {
            for item in &day.items {
                items.insert(item.key.clone(), item.clone());
            }
        }

        let merged_range = self.loaded_range.merge(other.loaded_range);
        let fetched_at = self.fetched_at.max(other.fetched_at);
        let local_offset = local_offset();

        let mut grouped: BTreeMap<time::Date, Vec<AgendaItem>> = BTreeMap::new();
        for item in items.into_values() {
            grouped
                .entry(item.local_date(local_offset))
                .or_default()
                .push(item);
        }

        let days = grouped
            .into_iter()
            .map(|(date, mut items)| {
                items.sort_by(compare_items);
                AgendaDay { date, items }
            })
            .collect();

        Self {
            loaded_range: merged_range,
            fetched_at,
            days,
        }
    }

    pub fn first_future_day_index(&self, today: time::Date) -> Option<usize> {
        self.days.iter().position(|day| day.date >= today)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AgendaDay {
    pub date: time::Date,
    pub items: Vec<AgendaItem>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AgendaItem {
    pub key: String,
    pub id: String,
    pub kind: ItemKind,
    pub title: String,
    pub context_name: String,
    #[serde(with = "time::serde::iso8601")]
    pub occurs_at: OffsetDateTime,
    pub html_url: Option<String>,
    pub completed: bool,
    pub submitted: bool,
    pub missing: bool,
    pub late: bool,
    pub needs_grading: bool,
    pub new_activity: bool,
    pub details: Option<String>,
}

impl AgendaItem {
    pub fn local_date(&self, local_offset: time::UtcOffset) -> time::Date {
        self.occurs_at.to_offset(local_offset).date()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct ItemKind(pub String);

impl ItemKind {
    pub fn label(&self) -> &str {
        match self.0.as_str() {
            "assignment" => "Assignment",
            "quiz" => "Quiz",
            "discussion_topic" => "Discussion",
            "announcement" => "Announcement",
            "wiki_page" => "Page",
            "planner_note" => "Note",
            "calendar_event" => "Event",
            "assessment_request" => "Peer Review",
            "sub_assignment" => "Checkpoint",
            "peer_review_sub_assignment" => "Review",
            _ => &self.0,
        }
    }
}

pub fn compare_items(left: &AgendaItem, right: &AgendaItem) -> std::cmp::Ordering {
    left.occurs_at
        .cmp(&right.occurs_at)
        .then_with(|| left.context_name.cmp(&right.context_name))
        .then_with(|| left.title.cmp(&right.title))
        .then_with(|| left.kind.0.cmp(&right.kind.0))
}

pub fn local_offset() -> time::UtcOffset {
    time::UtcOffset::current_local_offset().unwrap_or(time::UtcOffset::UTC)
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::macros::datetime;

    fn make_item(key: &str, date: OffsetDateTime) -> AgendaItem {
        AgendaItem {
            key: key.to_string(),
            id: key.to_string(),
            kind: ItemKind("assignment".to_string()),
            title: key.to_string(),
            context_name: "ctx".to_string(),
            occurs_at: date,
            html_url: None,
            completed: false,
            submitted: false,
            missing: false,
            late: false,
            needs_grading: false,
            new_activity: false,
            details: None,
        }
    }

    #[test]
    fn merge_deduplicates_by_key() {
        let old = AgendaSnapshot {
            loaded_range: DateRange::new(
                datetime!(2026-01-01 0:00 UTC).date(),
                datetime!(2026-01-03 0:00 UTC).date(),
            ),
            fetched_at: datetime!(2026-01-01 0:00 UTC),
            days: vec![AgendaDay {
                date: datetime!(2026-01-01 0:00 UTC).date(),
                items: vec![make_item("a", datetime!(2026-01-01 10:00 UTC))],
            }],
        };
        let mut replacement = make_item("a", datetime!(2026-01-01 11:00 UTC));
        replacement.completed = true;
        let new = AgendaSnapshot {
            loaded_range: DateRange::new(
                datetime!(2026-01-01 0:00 UTC).date(),
                datetime!(2026-01-04 0:00 UTC).date(),
            ),
            fetched_at: datetime!(2026-01-02 0:00 UTC),
            days: vec![AgendaDay {
                date: datetime!(2026-01-01 0:00 UTC).date(),
                items: vec![replacement],
            }],
        };

        let merged = old.merge(new);
        assert_eq!(merged.days.len(), 1);
        assert!(merged.days[0].items[0].completed);
        assert_eq!(
            merged.loaded_range.end,
            datetime!(2026-01-04 0:00 UTC).date()
        );
    }
}
