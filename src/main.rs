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
use std::sync::atomic::{AtomicU8, AtomicU16, Ordering};
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

    let (tx, mut rx) = mpsc::channel::<NodeData>(4);
    let current_screen = Arc::new(AtomicU8::new(Screen::Dashboard as u8));
    let poll_notify = Arc::new(Notify::new());
    let signaling_notify = Arc::new(Notify::new());

    let signaling_progress = Arc::new(AtomicU16::new(0));

    // Main poll loop: handles Dashboard and KnownPeers
    let poll_client = client.clone();
    let poll_screen = current_screen.clone();
    let poll_wake = poll_notify.clone();
    let poll_tx = tx.clone();
    let interval = Duration::from_secs(args.interval);
    tokio::spawn(async move {
        loop {
            let screen = Screen::from_u8(poll_screen.load(Ordering::Relaxed));
            let result = match screen {
                Screen::Dashboard => Some(poll_client.fetch_dashboard().await),
                Screen::KnownPeers => Some(poll_client.fetch_known_peers().await),
                Screen::Signaling => None, // handled by signaling task
            };
            if let Some(result) = result {
                match result {
                    Ok(data) => {
                        let _ = poll_tx.send(data).await;
                    }
                    Err(e) => {
                        let _ = poll_tx
                            .send(NodeData {
                                error: Some(format!("{}", e)),
                                ..Default::default()
                            })
                            .await;
                    }
                }
            }
            if screen == Screen::Dashboard {
                tokio::select! {
                    _ = tokio::time::sleep(interval) => {}
                    _ = poll_wake.notified() => {}
                }
            } else {
                poll_wake.notified().await;
            }
        }
    });

    // Separate signaling task: runs independently so it doesn't block dashboard
    let sig_client = client.clone();
    let sig_progress = signaling_progress.clone();
    let sig_wake = signaling_notify.clone();
    let sig_tx = tx.clone();
    tokio::spawn(async move {
        loop {
            sig_wake.notified().await;
            sig_progress.store(0, Ordering::Relaxed);
            let result = sig_client.fetch_signaling(&sig_progress, &sig_tx).await;
            match result {
                Ok(data) => {
                    let _ = sig_tx.send(data).await;
                }
                Err(e) => {
                    let _ = sig_tx
                        .send(NodeData {
                            error: Some(format!("{}", e)),
                            ..Default::default()
                        })
                        .await;
                }
            }
        }
    });

    enable_raw_mode()?;
    stdout().execute(EnterAlternateScreen)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout()))?;
    terminal.clear()?;

    let mut node_data = NodeData::default();
    let mut signaling_data = NodeData::default();
    let mut last_render = Instant::now();
    let mut peer_scroll: u16 = 0;
    let mut block_scroll: u16 = 0;
    let mut screen = Screen::Dashboard;
    let mut selected_bit: u8 = 0;
    let mut show_bit_modal = false;
    let mut signaling_loaded = false;
    let mut rpc_active_until = Instant::now();

    loop {
        while let Ok(data) = rx.try_recv() {
            if !data.recent_block_versions.is_empty() {
                signaling_loaded = true;
                signaling_data = data.clone();
            }
            // Always update node_data for non-signaling screens
            node_data = data;
            rpc_active_until = Instant::now() + Duration::from_millis(500);
        }

        if last_render.elapsed() >= Duration::from_millis(100) {
            let sig_progress = signaling_progress.load(Ordering::Relaxed);
            let rpc_active = Instant::now() < rpc_active_until || (screen == Screen::Signaling && !signaling_loaded && sig_progress > 0);
            let draw_data = if screen == Screen::Signaling && signaling_loaded {
                &signaling_data
            } else {
                &node_data
            };
            terminal.draw(|f| ui::draw(f, draw_data, peer_scroll, block_scroll, screen, selected_bit, show_bit_modal, signaling_loaded, sig_progress, rpc_active))?;
            last_render = Instant::now();
        }

        if event::poll(Duration::from_millis(50))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    if show_bit_modal {
                        match key.code {
                            KeyCode::Esc | KeyCode::Enter | KeyCode::Char('q') => {
                                show_bit_modal = false;
                            }
                            _ => {}
                        }
                    } else {
                        match key.code {
                            KeyCode::Char('q') | KeyCode::Esc => break,
                            KeyCode::Tab => {
                                screen = match screen {
                                    Screen::Dashboard => Screen::KnownPeers,
                                    Screen::KnownPeers => Screen::Signaling,
                                    Screen::Signaling => Screen::Dashboard,
                                };
                                current_screen.store(screen as u8, Ordering::Relaxed);
                                // Wake the main poll loop for Dashboard/KnownPeers
                                poll_notify.notify_one();
                            }
                            KeyCode::Down | KeyCode::Char('j') => {
                                if screen == Screen::Signaling {
                                    selected_bit = (selected_bit + 1).min(28);
                                } else {
                                    peer_scroll = peer_scroll.saturating_add(1);
                                }
                            }
                            KeyCode::Up | KeyCode::Char('k') => {
                                if screen == Screen::Signaling {
                                    selected_bit = selected_bit.saturating_sub(1);
                                } else {
                                    peer_scroll = peer_scroll.saturating_sub(1);
                                }
                            }
                            KeyCode::Enter => {
                                if screen == Screen::Signaling {
                                    show_bit_modal = true;
                                }
                            }
                            KeyCode::Char('r') => {
                                if screen == Screen::Signaling {
                                    signaling_notify.notify_one();
                                } else if screen == Screen::KnownPeers {
                                    poll_notify.notify_one();
                                }
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
    }

    // Drain any remaining key events so they don't leak to the shell
    while event::poll(Duration::from_millis(0))? {
        let _ = event::read();
    }
    terminal.clear()?;
    disable_raw_mode()?;
    stdout().execute(LeaveAlternateScreen)?;
    stdout().execute(crossterm::cursor::Show)?;
    println!();
    Ok(())
}
