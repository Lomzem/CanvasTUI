use std::fmt;

use crate::{
    config::Config,
    domain::{AgendaSnapshot, DateRange},
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FetchReason {
    Refresh,
    Older,
    Newer,
}

impl fmt::Display for FetchReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Refresh => write!(f, "refreshing"),
            Self::Older => write!(f, "loading older items"),
            Self::Newer => write!(f, "loading newer items"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FetchRequest {
    pub range: DateRange,
    pub reason: FetchReason,
}

#[derive(Clone, Debug)]
pub struct StatusLine {
    pub message: String,
    pub is_error: bool,
}

pub struct App {
    pub config: Config,
    pub snapshot: AgendaSnapshot,
    pub selected_day: usize,
    pub selected_item: usize,
    pub default_day: usize,
    pub should_quit: bool,
    pub status: StatusLine,
    in_flight: Vec<FetchRequest>,
}

impl App {
    pub fn new(config: Config, snapshot: AgendaSnapshot, today: time::Date) -> Self {
        let mut app = Self {
            config,
            snapshot,
            selected_day: 0,
            selected_item: 0,
            default_day: 0,
            should_quit: false,
            status: StatusLine {
                message: "Starting up".to_string(),
                is_error: false,
            },
            in_flight: Vec::new(),
        };
        app.reset_default_selection(today);
        app
    }

    pub fn reset_default_selection(&mut self, today: time::Date) {
        self.default_day = self
            .snapshot
            .first_future_day_index(today)
            .unwrap_or_else(|| self.snapshot.days.len().saturating_sub(1));
        if !self.snapshot.days.is_empty() {
            self.selected_day = self.default_day;
            self.selected_item = 0;
        }
    }

    pub fn current_day(&self) -> Option<&crate::domain::AgendaDay> {
        self.snapshot.days.get(self.selected_day)
    }

    pub fn current_item(&self) -> Option<&crate::domain::AgendaItem> {
        self.current_day()
            .and_then(|day| day.items.get(self.selected_item))
    }

    pub fn selected_item_key(&self) -> Option<String> {
        self.current_item().map(|item| item.key.clone())
    }

    pub fn set_status(&mut self, message: impl Into<String>, is_error: bool) {
        self.status = StatusLine {
            message: message.into(),
            is_error,
        };
    }

    pub fn mark_fetch_started(&mut self, request: FetchRequest) {
        if !self.in_flight.contains(&request) {
            self.set_status(request.reason.to_string(), false);
            self.in_flight.push(request);
        }
    }

    pub fn mark_fetch_finished(&mut self, request: &FetchRequest) {
        self.in_flight.retain(|existing| existing != request);
    }

    pub fn is_fetch_in_flight(&self, request: &FetchRequest) -> bool {
        self.in_flight.contains(request)
    }

    pub fn apply_snapshot_update(
        &mut self,
        incoming: AgendaSnapshot,
        today: time::Date,
        selected_key: Option<String>,
    ) {
        self.snapshot = self.snapshot.merge(incoming);
        self.default_day = self
            .snapshot
            .first_future_day_index(today)
            .unwrap_or_else(|| self.snapshot.days.len().saturating_sub(1));

        if let Some(key) = selected_key
            && let Some((day_idx, item_idx)) = self.find_item(&key)
        {
            self.selected_day = day_idx;
            self.selected_item = item_idx;
            return;
        }

        self.selected_day = self
            .selected_day
            .min(self.snapshot.days.len().saturating_sub(1));
        self.selected_item = self
            .current_day()
            .map(|day| self.selected_item.min(day.items.len().saturating_sub(1)))
            .unwrap_or(0);
    }

    pub fn previous_day_request(&self) -> Option<FetchRequest> {
        if self.selected_day > 0 || self.snapshot.days.is_empty() {
            return None;
        }

        Some(FetchRequest {
            range: self
                .snapshot
                .loaded_range
                .days_before(self.config.history_chunk_days),
            reason: FetchReason::Older,
        })
    }

    pub fn next_day_request(&self) -> Option<FetchRequest> {
        if self.snapshot.days.is_empty() || self.selected_day + 1 < self.snapshot.days.len() {
            return None;
        }

        Some(FetchRequest {
            range: self
                .snapshot
                .loaded_range
                .days_after(self.config.history_chunk_days),
            reason: FetchReason::Newer,
        })
    }

    pub fn refresh_request(&self) -> FetchRequest {
        FetchRequest {
            range: self.snapshot.loaded_range,
            reason: FetchReason::Refresh,
        }
    }

    pub fn jump_to_default_day(&mut self) {
        if !self.snapshot.days.is_empty() {
            self.selected_day = self.default_day;
            self.selected_item = 0;
        }
    }

    pub fn first_day(&mut self) {
        if !self.snapshot.days.is_empty() {
            self.selected_day = 0;
            self.selected_item = 0;
        }
    }

    pub fn last_day(&mut self) {
        if let Some(last_idx) = self.snapshot.days.len().checked_sub(1) {
            self.selected_day = last_idx;
            self.selected_item = 0;
        }
    }

    pub fn next_item(&mut self) {
        if let Some(day) = self.current_day() {
            if day.items.is_empty() {
                self.selected_item = 0;
            } else {
                self.selected_item = (self.selected_item + 1) % day.items.len();
            }
        }
    }

    pub fn previous_item(&mut self) {
        if let Some(day) = self.current_day() {
            if day.items.is_empty() {
                self.selected_item = 0;
            } else if self.selected_item == 0 {
                self.selected_item = day.items.len() - 1;
            } else {
                self.selected_item -= 1;
            }
        }
    }

    pub fn next_day(&mut self) -> bool {
        if self.selected_day + 1 < self.snapshot.days.len() {
            self.selected_day += 1;
            self.selected_item = 0;
            true
        } else {
            false
        }
    }

    pub fn previous_day(&mut self) -> bool {
        if self.selected_day > 0 {
            self.selected_day -= 1;
            self.selected_item = 0;
            true
        } else {
            false
        }
    }

    fn find_item(&self, key: &str) -> Option<(usize, usize)> {
        self.snapshot
            .days
            .iter()
            .enumerate()
            .find_map(|(day_idx, day)| {
                day.items
                    .iter()
                    .position(|item| item.key == key)
                    .map(|item_idx| (day_idx, item_idx))
            })
    }
}
