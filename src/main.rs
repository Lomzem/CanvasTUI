mod app;
mod cache;
mod canvas;
mod config;
mod domain;
mod tui;
mod ui;

use app::{App, FetchReason, FetchRequest};
use color_eyre::eyre::{Context, Result};
use crossterm::event::{KeyCode, KeyModifiers};
use domain::AgendaSnapshot;
use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};

struct FetchResult {
    request: FetchRequest,
    result: Result<AgendaSnapshot>,
}

#[tokio::main]
async fn main() -> Result<()> {
    color_eyre::install()?;
    run().await
}

async fn run() -> Result<()> {
    let local_offset = domain::local_offset();
    let today = time::OffsetDateTime::now_utc()
        .to_offset(local_offset)
        .date();
    let config = config::Config::load(today)?;
    let client = canvas::CanvasClient::new(config.canvas_url.clone(), config.access_token.clone());

    let cached_snapshot = cache::load(&config.cache_path).wrap_err("failed to load cache")?;
    let initial_snapshot = cached_snapshot.unwrap_or_else(|| {
        AgendaSnapshot::empty(config.initial_range, time::OffsetDateTime::now_utc())
    });
    let mut app = App::new(config.clone(), initial_snapshot, today);
    if app.snapshot.days.is_empty() {
        app.set_status("Loading Canvas planner items...", false);
    } else {
        app.set_status("Loaded cached planner snapshot; refreshing...", false);
    }

    let (fetch_tx, mut fetch_rx): (UnboundedSender<FetchResult>, UnboundedReceiver<FetchResult>) =
        mpsc::unbounded_channel();
    let initial_request = app.refresh_request();
    request_fetch(&client, &mut app, fetch_tx.clone(), initial_request);

    let mut tui = tui::Tui::new()?;
    tui.enter()?;

    loop {
        tokio::select! {
            event = tui.next() => {
                match event? {
                    tui::Event::Render => {
                        tui.draw(|frame| ui::draw(frame, &app))?;
                    }
                    tui::Event::Key(key) => handle_key(&client, &mut app, &fetch_tx, today, key),
                    tui::Event::Error => app.set_status("Terminal input error", true),
                    tui::Event::Tick => {}
                }
            }
            Some(fetch_result) = fetch_rx.recv() => {
                handle_fetch_result(&mut app, today, fetch_result)?;
            }
        }

        if app.should_quit {
            break;
        }
    }

    tui.exit()?;
    Ok(())
}

fn handle_key(
    client: &canvas::CanvasClient,
    app: &mut App,
    fetch_tx: &UnboundedSender<FetchResult>,
    today: time::Date,
    key: crossterm::event::KeyEvent,
) {
    match key.code {
        KeyCode::Char('q') => app.should_quit = true,
        KeyCode::Char('j') => app.next_item(),
        KeyCode::Char('k') => app.previous_item(),
        KeyCode::Char('h') => {
            if !app.previous_day()
                && let Some(request) = app.previous_day_request()
            {
                request_fetch(client, app, fetch_tx.clone(), request);
            }
        }
        KeyCode::Char('l') => {
            if !app.next_day()
                && let Some(request) = app.next_day_request()
            {
                request_fetch(client, app, fetch_tx.clone(), request);
            }
        }
        KeyCode::Char('g') if key.modifiers.contains(KeyModifiers::SHIFT) => app.last_day(),
        KeyCode::Char('g') => app.first_day(),
        KeyCode::Char('0') => app.jump_to_default_day(),
        KeyCode::Char('r') => request_fetch(client, app, fetch_tx.clone(), app.refresh_request()),
        KeyCode::Char('o') => {
            if let Some(url) = app.current_item().and_then(|item| item.html_url.as_ref()) {
                match webbrowser::open(url) {
                    Ok(_) => app.set_status(format!("Opened {url}"), false),
                    Err(error) => app.set_status(format!("Failed to open URL: {error}"), true),
                }
            } else {
                app.set_status("Selected item does not provide an openable URL", true);
            }
        }
        _ => {}
    }

    let _ = today;
}

fn request_fetch(
    client: &canvas::CanvasClient,
    app: &mut App,
    fetch_tx: UnboundedSender<FetchResult>,
    request: FetchRequest,
) {
    if app.is_fetch_in_flight(&request) {
        return;
    }
    app.mark_fetch_started(request.clone());
    let client = client.clone();
    tokio::spawn(async move {
        let result = client.fetch_snapshot(request.range).await;
        let _ = fetch_tx.send(FetchResult { request, result });
    });
}

fn handle_fetch_result(app: &mut App, today: time::Date, fetch_result: FetchResult) -> Result<()> {
    let selected_key = app.selected_item_key();
    app.mark_fetch_finished(&fetch_result.request);
    match fetch_result.result {
        Ok(snapshot) => {
            let fetched_at = snapshot.fetched_at;
            app.apply_snapshot_update(snapshot, today, selected_key);
            cache::store(&app.config.cache_path, &app.snapshot)
                .wrap_err("failed to store cache")?;
            match fetch_result.request.reason {
                FetchReason::Refresh => app.set_status(
                    format!(
                        "Planner refreshed at {}",
                        fetched_at.format(&time::format_description::parse("[hour]:[minute]")?)?
                    ),
                    false,
                ),
                FetchReason::Older => {
                    if app.selected_day > 0 {
                        app.previous_day();
                    }
                    app.set_status("Loaded older planner items", false);
                }
                FetchReason::Newer => {
                    if app.selected_day + 1 < app.snapshot.days.len() {
                        app.next_day();
                    }
                    app.set_status("Loaded newer planner items", false);
                }
            }
        }
        Err(error) => {
            app.set_status(format!("{}: {error}", fetch_result.request.reason), true);
        }
    }
    Ok(())
}
