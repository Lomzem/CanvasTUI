mod fetch;
mod tui;

use crossterm::event::KeyCode::Char;

use color_eyre::eyre::Result;
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout},
    prelude::{Buffer, Rect},
    style::{Color, Style, Stylize},
    widgets::{
        Block, BorderType, Borders, Cell, Padding, Paragraph, Row, StatefulWidget, Table, Widget,
    },
};
use tokio::sync::mpsc::{self};
use tui::Event;

use crate::fetch::{Calendar, fetch};

const CACHE_FILE: &str = "/tmp/canvastui.json";

struct App {
    calendar: Calendar,
    should_quit: bool,
    // action_tx: UnboundedSender<Action>,
    longest_item_lens: (u16, u16, u16),
    received_fetch: bool,
}

#[derive(Clone)]
pub enum Action {
    Tick,
    FetchComplete(Calendar),
    FileFetchComplete(Calendar),
    Quit,
    Render,
    NextEvent,
    PrevEvent,
    NextDate,
    PrevDate,
    OpenURL,
    None,
}

impl App {
    pub fn calculate_longest_item_lens(&mut self) {
        self.calendar.dates.iter().for_each(|date| {
            date.events.iter().for_each(|event| {
                let course_name_len = event.course_name.len() as u16;
                let title_len = event.title.len() as u16;
                let due_at_len = event.due_at.format("  %H:%M").to_string().len() as u16;
                self.longest_item_lens = (
                    course_name_len.max(self.longest_item_lens.0) + 1,
                    title_len.max(self.longest_item_lens.1),
                    due_at_len.max(self.longest_item_lens.2),
                );
            });
        });
    }
}

impl Widget for &mut App {
    fn render(self, area: Rect, buf: &mut Buffer)
    where
        Self: Sized,
    {
        if self.calendar.dates.is_empty() {
            Paragraph::new("Waiting for data...").render(area, buf);
            return;
        }

        let [date_area, event_table_area] =
            Layout::vertical([Constraint::Length(1), Constraint::Fill(1)]).areas(area);

        let current_date = &mut self.calendar.dates[self.calendar.current_date_index];
        Paragraph::new(
            current_date
                .events
                .first()
                .unwrap()
                .due_at
                .format("%A %b %-d")
                .to_string(),
        )
        .style(Style::default().fg(Color::Magenta).bold())
        .render(date_area, buf);

        let header = ["Course", "Assignment", "Due"]
            .into_iter()
            .map(Cell::from)
            .collect::<Row>()
            .height(1)
            .style(Style::default().fg(Color::Magenta));
        let rows = current_date.events.iter().map(|e| {
            Row::new([
                Cell::from(e.course_name.to_string()),
                Cell::from(e.title.to_string()),
                match e.submitted {
                    true => Cell::from(e.due_at.format("%H:%M 󰸞").to_string()),
                    false => Cell::from(e.due_at.format("%H:%M  ").to_string()),
                },
            ])
            .style(Style::default().fg(match e.submitted {
                true => Color::Green,
                false => Color::White,
            }))
        });
        let event_table = Table::new(
            rows,
            [
                Constraint::Length(self.longest_item_lens.0 + 1),
                Constraint::Min(self.longest_item_lens.1.max("Assignment".len() as u16) + 2),
                Constraint::Length(self.longest_item_lens.2 + 2),
            ],
        )
        .header(header)
        .row_highlight_style(Style::default().bg(Color::Black))
        .style(Style::default().fg(Color::White));
        StatefulWidget::render(
            event_table,
            event_table_area,
            buf,
            &mut current_date.table_state,
        )
    }
}

fn ui(frame: &mut Frame, app: &mut App) {
    let block = Block::default()
        .title(" CanvasTUI ")
        .title_alignment(Alignment::Center)
        .borders(Borders::ALL)
        .border_type(BorderType::Thick)
        .style(Style::default().fg(Color::Blue))
        .padding(Padding::horizontal(1))
        .title_alignment(Alignment::Center);
    let block_area = block.inner(frame.area());
    block.render(frame.area(), frame.buffer_mut());
    app.render(block_area, frame.buffer_mut())
}

fn get_action(_app: &App, event: Event) -> Action {
    match event {
        Event::Error => Action::None,
        Event::Tick => Action::Tick,
        Event::Render => Action::Render,
        Event::Key(key) => match key.code {
            Char('q') => Action::Quit,
            Char('k') => Action::PrevEvent,
            Char('j') => Action::NextEvent,
            Char('h') => Action::PrevDate,
            Char('l') => Action::NextDate,
            Char('o') => Action::OpenURL,
            _ => Action::None,
        },
    }
}

fn update(app: &mut App, action: Action) {
    match action {
        Action::Quit => app.should_quit = true,
        Action::FetchComplete(data) => {
            app.calendar = data;
            app.received_fetch = true;
            app.calculate_longest_item_lens();
            app.calendar.current_date_index = 0;
        }
        Action::FileFetchComplete(data) => {
            if app.received_fetch {
                return;
            }
            app.calendar = data;
            app.calculate_longest_item_lens();
        }
        Action::Tick => {}
        Action::Render => {}
        Action::PrevEvent => {
            if let Some(current_date) = app.calendar.dates.get_mut(app.calendar.current_date_index)
            {
                current_date.table_state.select_previous();
            }
        }
        Action::NextEvent => {
            if let Some(current_date) = app.calendar.dates.get_mut(app.calendar.current_date_index)
            {
                current_date.table_state.select_next();
            }
        }
        Action::NextDate => {
            app.calendar.current_date_index = app
                .calendar
                .current_date_index
                .saturating_add(1)
                .min(app.calendar.dates.len().saturating_sub(1));
        }
        Action::PrevDate => {
            app.calendar.current_date_index =
                app.calendar.current_date_index.saturating_sub(1).max(0);
        }
        Action::OpenURL => {
            let selected_idx = app.calendar.dates[app.calendar.current_date_index]
                .table_state
                .selected()
                .expect("Something should always be selected from list");
            let selected_event =
                &app.calendar.dates[app.calendar.current_date_index].events[selected_idx];
            webbrowser::open(&selected_event.html_url).unwrap();
        }
        Action::None => {}
    };
}

async fn run() -> Result<()> {
    let (action_tx, mut action_rx) = mpsc::unbounded_channel(); // new

    {
        let action_tx = action_tx.clone();
        tokio::spawn(async move {
            let cached_body_bytes = tokio::fs::read(CACHE_FILE).await.unwrap();
            let calendar: Calendar = serde_json::from_slice(&cached_body_bytes).unwrap();
            action_tx.send(Action::FileFetchComplete(calendar)).unwrap();
        });
    }
    {
        let mut action_tx = action_tx.clone();
        tokio::spawn(async move {
            fetch(&mut action_tx).await.unwrap();
        });
    }

    let mut tui = tui::Tui::new()?;
    tui.enter()?;

    let mut app = App {
        should_quit: false,
        // action_tx: action_tx.clone(),
        longest_item_lens: (0, 0, 0),
        received_fetch: false,
        calendar: Calendar {
            current_date_index: 0,
            dates: vec![],
        },
    };

    loop {
        let e = tui.next().await?;
        match e {
            tui::Event::Tick => action_tx.send(Action::Tick)?,
            tui::Event::Render => action_tx.send(Action::Render)?,
            tui::Event::Key(_) => {
                let action = get_action(&app, e);
                action_tx.send(action.clone())?;
            }
            _ => {}
        };

        while let Ok(action) = action_rx.try_recv() {
            update(&mut app, action.clone());
            if let Action::Render = action {
                tui.draw(|f| {
                    ui(f, &mut app);
                })?;
            }
        }

        if app.should_quit {
            break;
        }
    }
    tui.exit()?;

    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    color_eyre::install()?;
    let result = run().await;

    result?;

    Ok(())
}
