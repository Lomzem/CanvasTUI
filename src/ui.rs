use ratatui::{
    Frame,
    layout::{Constraint, Layout, Margin},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Cell, Clear, Paragraph, Row, Table, TableState, Wrap},
};
use time::format_description;

use crate::app::App;

pub fn draw(frame: &mut Frame, app: &App) {
    let area = frame.area();
    let block = Block::bordered()
        .title(" CanvasTUI ")
        .border_style(Style::default().fg(Color::Blue));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let [header_area, table_area, footer_area] = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(5),
        Constraint::Length(2),
    ])
    .areas(inner);

    render_header(frame, app, header_area);
    render_body(frame, app, table_area);
    render_footer(frame, app, footer_area);
}

fn render_header(frame: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    let selected_day = app.current_day();
    let title = selected_day
        .map(|day| {
            day.date
                .format(
                    &format_description::parse(
                        "[weekday repr:long], [month repr:long] [day], [year]",
                    )
                    .unwrap(),
                )
                .unwrap()
        })
        .unwrap_or_else(|| "No planner items loaded".to_string());
    let summary = selected_day
        .map(|day| format!("{} items", day.items.len()))
        .unwrap_or_else(|| "Waiting for Canvas data".to_string());
    let paragraph = Paragraph::new(vec![
        Line::from(vec![Span::styled(
            title,
            Style::default()
                .fg(Color::Magenta)
                .add_modifier(Modifier::BOLD),
        )]),
        Line::from(summary),
    ]);
    frame.render_widget(paragraph, area);
}

fn render_body(frame: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    if app.snapshot.days.is_empty() {
        let help = Paragraph::new("No cached items yet. Waiting for the first Canvas fetch.")
            .wrap(Wrap { trim: true });
        frame.render_widget(help, area.inner(Margin::new(1, 1)));
        return;
    }

    let day = match app.current_day() {
        Some(day) => day,
        None => return,
    };

    let header = Row::new([" ", "Time", "Type", "Context", "Title", "Flags"]).style(
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    );
    let rows = day.items.iter().map(|item| {
        let status = if item.completed || item.submitted {
            "✓"
        } else {
            " "
        };
        let time = item
            .occurs_at
            .format(&format_description::parse("[hour]:[minute]").unwrap())
            .unwrap_or_else(|_| "??:??".to_string());
        let flags = item_flags(item);
        let mut style = Style::default();
        if item.completed || item.submitted {
            style = style.fg(Color::Green);
        } else if item.missing {
            style = style.fg(Color::Red);
        } else if item.late {
            style = style.fg(Color::Yellow);
        }

        Row::new([
            Cell::from(status),
            Cell::from(time),
            Cell::from(item.kind.label()),
            Cell::from(item.context_name.as_str()),
            Cell::from(item.title.as_str()),
            Cell::from(flags),
        ])
        .style(style)
    });

    let table = Table::new(
        rows,
        [
            Constraint::Length(2),
            Constraint::Length(6),
            Constraint::Length(13),
            Constraint::Length(22),
            Constraint::Fill(1),
            Constraint::Length(14),
        ],
    )
    .header(header)
    .row_highlight_style(
        Style::default()
            .bg(Color::DarkGray)
            .add_modifier(Modifier::BOLD),
    );

    let mut state = TableState::default().with_selected(Some(app.selected_item));
    frame.render_stateful_widget(table, area, &mut state);

    if let Some(item) = app.current_item().filter(|item| item.details.is_some()) {
        let popup_area = area.inner(Margin::new(1, 1));
        let popup = Block::bordered().title(" Details ");
        let inner = popup.inner(popup_area);
        frame.render_widget(Clear, popup_area);
        frame.render_widget(popup, popup_area);
        frame.render_widget(
            Paragraph::new(item.details.as_deref().unwrap_or("")).wrap(Wrap { trim: true }),
            inner,
        );
    }
}

fn render_footer(frame: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    let mut status_style = Style::default().fg(Color::Gray);
    if app.status.is_error {
        status_style = status_style.fg(Color::Red);
    }
    let open_hint = if app
        .current_item()
        .and_then(|item| item.html_url.as_ref())
        .is_some()
    {
        "o open"
    } else {
        "o unavailable"
    };
    let lines = vec![
        Line::from(vec![Span::styled(
            app.status.message.as_str(),
            status_style,
        )]),
        Line::from(
            "j/k move item  h/l move day  g/G ends  0 default  r refresh  o open  q quit"
                .replace("o open", open_hint),
        ),
    ];
    frame.render_widget(Paragraph::new(lines), area);
}

fn item_flags(item: &crate::domain::AgendaItem) -> String {
    let mut flags = Vec::new();
    if item.new_activity {
        flags.push("new");
    }
    if item.needs_grading {
        flags.push("grade");
    }
    if item.missing {
        flags.push("missing");
    }
    if item.late {
        flags.push("late");
    }
    if flags.is_empty() {
        String::new()
    } else {
        flags.join(",")
    }
}
