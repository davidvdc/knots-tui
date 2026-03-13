mod rpc;
mod ui;

use clap::Parser;
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    ExecutableCommand,
};
use ratatui::prelude::*;
use std::io::stdout;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, Notify};

use rpc::{NodeData, RpcClient};

#[derive(Clone, Copy, PartialEq)]
pub enum Screen {
    Dashboard = 0,
    KnownPeers = 1,
    Signaling = 2,
}

impl Screen {
    fn from_u8(v: u8) -> Self {
        match v {
            1 => Screen::KnownPeers,
            2 => Screen::Signaling,
            _ => Screen::Dashboard,
        }
    }
}

#[derive(Parser)]
#[command(name = "knots-tui", about = "Bitcoin Knots node dashboard")]
struct Args {
    /// RPC URL (e.g. http://192.168.1.50:8332)
    #[arg(long, env = "KNOTS_RPC_URL", default_value = "http://127.0.0.1:8332")]
    rpc_url: String,

    /// Path to .cookie file for authentication
    #[arg(long, env = "KNOTS_COOKIE_FILE", default_value = "~/.bitcoin/.cookie")]
    cookie_file: String,

    /// Refresh interval in seconds
    #[arg(long, default_value = "5")]
    interval: u64,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    let cookie_path = shellexpand::tilde(&args.cookie_file).to_string();
    let cookie = match std::fs::read_to_string(&cookie_path) {
        Ok(c) => c.trim().to_string(),
        Err(e) => {
            eprintln!("Failed to read cookie file '{}': {}", cookie_path, e);
            eprintln!("Provide the path via --cookie-file or KNOTS_COOKIE_FILE env var.");
            std::process::exit(1);
        }
    };

    let client = RpcClient::new(&args.rpc_url, &cookie);

    let (tx, mut rx) = mpsc::channel::<NodeData>(2);
    let current_screen = Arc::new(AtomicU8::new(Screen::Dashboard as u8));
    let poll_notify = Arc::new(Notify::new());

    let poll_client = client.clone();
    let poll_screen = current_screen.clone();
    let poll_wake = poll_notify.clone();
    let interval = Duration::from_secs(args.interval);
    tokio::spawn(async move {
        loop {
            let screen = Screen::from_u8(poll_screen.load(Ordering::Relaxed));
            let result = match screen {
                Screen::Dashboard => poll_client.fetch_dashboard().await,
                Screen::KnownPeers => poll_client.fetch_known_peers().await,
                Screen::Signaling => poll_client.fetch_signaling().await,
            };
            match result {
                Ok(data) => {
                    let _ = tx.send(data).await;
                }
                Err(e) => {
                    let _ = tx
                        .send(NodeData {
                            error: Some(format!("{}", e)),
                            ..Default::default()
                        })
                        .await;
                }
            }
            tokio::select! {
                _ = tokio::time::sleep(interval) => {}
                _ = poll_wake.notified() => {}
            }
        }
    });

    enable_raw_mode()?;
    stdout().execute(EnterAlternateScreen)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout()))?;
    terminal.clear()?;

    let mut node_data = NodeData::default();
    let mut last_render = Instant::now();
    let mut peer_scroll: u16 = 0;
    let mut block_scroll: u16 = 0;
    let mut screen = Screen::Dashboard;

    loop {
        while let Ok(data) = rx.try_recv() {
            node_data = data;
        }

        if last_render.elapsed() >= Duration::from_millis(100) {
            terminal.draw(|f| ui::draw(f, &node_data, peer_scroll, block_scroll, screen))?;
            last_render = Instant::now();
        }

        if event::poll(Duration::from_millis(50))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    match key.code {
                        KeyCode::Char('q') | KeyCode::Esc => break,
                        KeyCode::Tab => {
                            screen = match screen {
                                Screen::Dashboard => Screen::KnownPeers,
                                Screen::KnownPeers => Screen::Signaling,
                                Screen::Signaling => Screen::Dashboard,
                            };
                            current_screen.store(screen as u8, Ordering::Relaxed);
                            poll_notify.notify_one();
                        }
                        KeyCode::Down | KeyCode::Char('j') => {
                            peer_scroll = peer_scroll.saturating_add(1);
                        }
                        KeyCode::Up | KeyCode::Char('k') => {
                            peer_scroll = peer_scroll.saturating_sub(1);
                        }
                        KeyCode::Char('J') => {
                            block_scroll = block_scroll.saturating_add(1);
                        }
                        KeyCode::Char('K') => {
                            block_scroll = block_scroll.saturating_sub(1);
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    disable_raw_mode()?;
    stdout().execute(LeaveAlternateScreen)?;
    Ok(())
}
