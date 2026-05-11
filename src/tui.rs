use crossterm::{
    cursor,
    event::{Event as CrosstermEvent, EventStream, KeyEvent, KeyEventKind},
    terminal::{EnterAlternateScreen, LeaveAlternateScreen},
};
use futures::{FutureExt, StreamExt};
use ratatui::backend::CrosstermBackend;
use std::{
    io::{Stderr, stderr},
    ops::{Deref, DerefMut},
    time::Duration,
};
use tokio::{
    sync::mpsc::{self, UnboundedReceiver, UnboundedSender},
    task::JoinHandle,
};
use tokio_util::sync::CancellationToken;

use color_eyre::eyre::{Result, eyre};

#[derive(Clone, Debug)]
pub enum Event {
    Tick,
    Render,
    Key(KeyEvent),
    Error,
}

pub struct Tui {
    terminal: ratatui::Terminal<CrosstermBackend<Stderr>>,
    task: JoinHandle<()>,
    cancellation_token: CancellationToken,
    event_rx: UnboundedReceiver<Event>,
    event_tx: UnboundedSender<Event>,
    tick_rate: f64,
    frame_rate: f64,
}

impl Tui {
    pub fn new() -> Result<Self> {
        let terminal = ratatui::Terminal::new(CrosstermBackend::new(stderr()))?;
        let (event_tx, event_rx) = mpsc::unbounded_channel();
        Ok(Self {
            terminal,
            task: tokio::spawn(async {}),
            cancellation_token: CancellationToken::new(),
            event_rx,
            event_tx,
            tick_rate: 4.0,
            frame_rate: 30.0,
        })
    }

    pub fn enter(&mut self) -> Result<()> {
        crossterm::terminal::enable_raw_mode()?;
        crossterm::execute!(stderr(), EnterAlternateScreen, cursor::Hide)?;
        self.start();
        Ok(())
    }

    pub fn exit(&mut self) -> Result<()> {
        self.cancel();
        if crossterm::terminal::is_raw_mode_enabled()? {
            crossterm::execute!(stderr(), LeaveAlternateScreen, cursor::Show)?;
            crossterm::terminal::disable_raw_mode()?;
        }
        Ok(())
    }

    pub async fn next(&mut self) -> Result<Event> {
        self.event_rx
            .recv()
            .await
            .ok_or_else(|| eyre!("failed to receive terminal event"))
    }

    fn start(&mut self) {
        self.cancel();
        self.cancellation_token = CancellationToken::new();
        let cancellation_token = self.cancellation_token.clone();
        let event_tx = self.event_tx.clone();
        let tick_delay = Duration::from_secs_f64(1.0 / self.tick_rate);
        let render_delay = Duration::from_secs_f64(1.0 / self.frame_rate);

        self.task = tokio::spawn(async move {
            let mut reader = EventStream::new();
            let mut tick_interval = tokio::time::interval(tick_delay);
            let mut render_interval = tokio::time::interval(render_delay);

            loop {
                let tick = tick_interval.tick();
                let render = render_interval.tick();
                let event = reader.next().fuse();

                tokio::select! {
                    _ = cancellation_token.cancelled() => break,
                    maybe_event = event => match maybe_event {
                        Some(Ok(CrosstermEvent::Key(key))) if key.kind == KeyEventKind::Press => {
                            let _ = event_tx.send(Event::Key(key));
                        }
                        Some(Ok(_)) => {}
                        Some(Err(_)) => {
                            let _ = event_tx.send(Event::Error);
                        }
                        None => {}
                    },
                    _ = tick => {
                        let _ = event_tx.send(Event::Tick);
                    }
                    _ = render => {
                        let _ = event_tx.send(Event::Render);
                    }
                }
            }
        });
    }

    fn cancel(&self) {
        self.cancellation_token.cancel();
    }
}

impl Deref for Tui {
    type Target = ratatui::Terminal<CrosstermBackend<Stderr>>;

    fn deref(&self) -> &Self::Target {
        &self.terminal
    }
}

impl DerefMut for Tui {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.terminal
    }
}

impl Drop for Tui {
    fn drop(&mut self) {
        let _ = self.exit();
    }
}
