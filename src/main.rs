mod tui;

use chrono::{DateTime, Local, NaiveDate};
use crossterm::event::KeyCode::Char;
use std::time::Duration;

use color_eyre::{eyre::Result, owo_colors::OwoColorize};
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout},
    prelude::{Buffer, Rect},
    style::{Color, Style},
    widgets::{
        Block, BorderType, Borders, List, ListItem, ListState, Paragraph, StatefulWidget, Widget,
    },
};
use tokio::sync::mpsc::{self, UnboundedSender};
use tui::Event;

struct Calendar {
    pub dates: Vec<CalendarDate>,
    pub current_date_index: usize,
}

struct CalendarDate {
    pub date: NaiveDate,
    pub events: Vec<CalendarEvent>,
    pub list_state: ListState,
}

struct CalendarEvent {
    pub course_name: String,
    pub due_at: DateTime<Local>,
    pub title: String,
    pub html_url: String,
    pub submitted: bool,
}

struct App {
    calendar: Calendar,
    should_quit: bool,
    action_tx: UnboundedSender<Action>,
}

#[derive(Clone)]
pub enum Action {
    Tick,
    FetchComplete(i64),
    Quit,
    Render,
    NextEvent,
    PrevEvent,
    NextDate,
    PrevDate,
    None,
}

impl Widget for &mut Calendar {
    fn render(self, area: Rect, buf: &mut Buffer)
    where
        Self: Sized,
    {
        let [date_area, event_list_area] =
            Layout::vertical([Constraint::Length(1), Constraint::Fill(1)]).areas(area);

        let current_date = &mut self.dates[self.current_date_index];
        Paragraph::new(format!(
            "Current Date: {}",
            current_date.date.format("%Y-%m-%d")
        ))
        .style(Style::default().fg(Color::White))
        .render(date_area, buf);
        let event_list = List::new(
            current_date
                .events
                .iter()
                .map(|e| ListItem::from(e.title.as_str())),
        )
        .highlight_style(Style::default().bg(Color::Black).fg(Color::White));
        StatefulWidget::render(
            event_list,
            event_list_area,
            buf,
            &mut current_date.list_state,
        )
    }
}

fn ui(frame: &mut Frame, app: &mut App) {
    let block = Block::default()
        .title(" CanvasTUI ")
        .title_alignment(Alignment::Center)
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .style(Style::default().fg(Color::Red))
        .title_alignment(Alignment::Center);
    let block_area = block.inner(frame.area());
    block.render(frame.area(), frame.buffer_mut());
    app.calendar.render(block_area, frame.buffer_mut());
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
            _ => Action::None,
        },
        Event::Quit => Action::None,
        Event::Closed => todo!(),
    }
}

fn update(app: &mut App, action: Action) {
    match action {
        Action::Quit => app.should_quit = true,
        Action::FetchComplete(data) => todo!(),
        Action::Tick => {}
        Action::Render => {}
        Action::PrevEvent => {
            app.calendar.dates[app.calendar.current_date_index]
                .list_state
                .select_previous();
        }
        Action::NextEvent => {
            app.calendar.dates[app.calendar.current_date_index]
                .list_state
                .select_next();
        }
        Action::None => {}
        Action::NextDate => {
            app.calendar.current_date_index = app
                .calendar
                .current_date_index
                .saturating_add(1)
                .min(app.calendar.dates.len() - 1);
        }
        Action::PrevDate => {
            app.calendar.current_date_index =
                app.calendar.current_date_index.saturating_sub(1).max(0);
        }
    };
}

async fn run() -> Result<()> {
    let (action_tx, mut action_rx) = mpsc::unbounded_channel(); // new

    // {
    //     let action_tx = action_tx.clone();
    //     tokio::spawn(async move {
    //         tokio::time::sleep(Duration::from_secs(2)).await; // simulate network request
    //         action_tx.send(Action::FetchComplete(67)).unwrap();
    //     });
    // }

    let mut tui = tui::Tui::new()?;
    tui.enter()?;

    let mut app = App {
        should_quit: false,
        action_tx: action_tx.clone(),
        calendar: Calendar {
            current_date_index: 0,
            dates: vec![
                CalendarDate {
                    date: NaiveDate::from_ymd_opt(2024, 1, 1).unwrap(),
                    events: vec![
                        CalendarEvent {
                            course_name: "foo".to_string(),
                            due_at: Local::now(),
                            title: "foo".to_string(),
                            html_url: "foo".to_string(),
                            submitted: false,
                        },
                        CalendarEvent {
                            course_name: "foo2".to_string(),
                            due_at: Local::now(),
                            title: "foo2".to_string(),
                            html_url: "foo2".to_string(),
                            submitted: true,
                        },
                    ],
                    list_state: ListState::default().with_selected(Some(0)),
                },
                CalendarDate {
                    date: NaiveDate::from_ymd_opt(2024, 1, 2).unwrap(),
                    events: vec![
                        CalendarEvent {
                            course_name: "foo".to_string(),
                            due_at: Local::now(),
                            title: "foo3".to_string(),
                            html_url: "foo".to_string(),
                            submitted: false,
                        },
                        CalendarEvent {
                            course_name: "foo2".to_string(),
                            due_at: Local::now(),
                            title: "foo4".to_string(),
                            html_url: "foo2".to_string(),
                            submitted: true,
                        },
                    ],
                    list_state: ListState::default().with_selected(Some(0)),
                },
            ],
        },
    };

    loop {
        let e = tui.next().await?;
        match e {
            tui::Event::Quit => action_tx.send(Action::Quit)?,
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
    let result = run().await;

    result?;

    Ok(())
}
