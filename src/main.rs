mod rpc;
mod ui;

use clap::Parser;
use crossterm::{
    event::{Event, EventStream, KeyCode, KeyEventKind},
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    ExecutableCommand,
};
use futures::StreamExt;
use ratatui::prelude::*;
use std::collections::HashMap;
use std::io::stdout;
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU16, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, Notify};

use rpc::{BlockStats, NodeData, RpcClient};

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

#[tokio::main(flavor = "current_thread")]
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
    let force_full_fetch = Arc::new(AtomicBool::new(false));
    let spinner_notify = Arc::new(Notify::new());

    // Main poll loop: handles Dashboard and KnownPeers
    // Dashboard uses cheap quick-checks every `interval` seconds,
    // full fetch only on changes or every 60s.
    let poll_client = client.clone();
    let poll_screen = current_screen.clone();
    let poll_wake = poll_notify.clone();
    let poll_tx = tx.clone();
    let poll_force = force_full_fetch.clone();
    let poll_spinner = spinner_notify.clone();
    let quick_interval = Duration::from_secs(args.interval);
    let full_interval = Duration::from_secs(60);
    tokio::spawn(async move {
        let mut last_height: u64 = 0;
        let mut last_conns: u64 = 0;
        let mut last_full_fetch = tokio::time::Instant::now();
        let mut force_full = true; // first iteration always does full fetch
        loop {
            let screen = Screen::from_u8(poll_screen.load(Ordering::Relaxed));
            if poll_force.swap(false, Ordering::Relaxed) {
                force_full = true;
            }
            let result = match screen {
                Screen::Dashboard => {
                    let mut need_full = force_full;
                    force_full = false;

                    if !need_full {
                        // Cheap check: did block height or peer count change?
                        if let Ok((h, c)) = poll_client.fetch_tip_and_peers().await {
                            if h != last_height || c != last_conns {
                                need_full = true;
                            }
                        }
                    }

                    // Also do full fetch if 60s elapsed
                    if !need_full && last_full_fetch.elapsed() >= full_interval {
                        need_full = true;
                    }

                    if need_full {
                        let r = poll_client.fetch_dashboard().await;
                        if let Ok(ref data) = r {
                            last_height = data.blockchain.blocks;
                            last_conns = data.network.connections;
                            last_full_fetch = tokio::time::Instant::now();
                        }
                        Some(r)
                    } else {
                        poll_spinner.notify_one();
                        None
                    }
                }
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
                    _ = tokio::time::sleep(quick_interval) => {}
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
            let result = sig_client.fetch_signaling(&sig_progress).await;
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

    let (stats_tx, mut stats_rx) = mpsc::channel::<Vec<BlockStats>>(4);
    let stats_client = client.clone();

    let mut node_data = NodeData::default();
    let mut signaling_data = NodeData::default();
    let mut block_stats_cache: HashMap<u64, BlockStats> = HashMap::new();
    let mut last_tip_height: u64 = 0;
    let mut peer_scroll: u16 = 0;
    let mut selected_block: u8 = 0;
    let mut show_block_modal = false;
    let mut blocks_focused = true; // true = blocks table, false = peers table
    let mut screen = Screen::Dashboard;
    let mut selected_bit: u8 = 0;
    let mut show_bit_modal = false;
    let mut rpc_spinner: u8 = 0;

    let mut event_stream = EventStream::new();

    // Initial render
    terminal.draw(|f| ui::draw(f, &node_data, peer_scroll, screen, selected_bit, show_bit_modal, rpc_spinner, &block_stats_cache, selected_block, show_block_modal, blocks_focused))?;

    loop {
        let mut redraw = false;

        // Wait for: channel data, block stats, or keyboard event
        tokio::select! {
            Some(data) = rx.recv() => {
                if !data.recent_block_versions.is_empty() {
                    signaling_data = data.clone();
                }
                // Auto-fetch stats for newly mined blocks
                let new_tip = data.recent_blocks.first().map(|b| b.height).unwrap_or(0);
                if new_tip > last_tip_height && last_tip_height > 0 {
                    let new_blocks: Vec<(u64, String)> = data
                        .recent_blocks
                        .iter()
                        .filter(|b| b.height > last_tip_height && !block_stats_cache.contains_key(&b.height))
                        .map(|b| (b.height, b.hash.clone()))
                        .collect();
                    if !new_blocks.is_empty() {
                        let c = stats_client.clone();
                        let tx = stats_tx.clone();
                        tokio::spawn(async move {
                            if let Ok(stats) = c.fetch_block_stats(&new_blocks).await {
                                let _ = tx.send(stats).await;
                            }
                        });
                    }
                }
                last_tip_height = new_tip;
                node_data = data;
                rpc_spinner = rpc_spinner.wrapping_add(1);
                redraw = true;
            }
            _ = spinner_notify.notified() => {
                rpc_spinner = rpc_spinner.wrapping_add(1);
                redraw = true;
            }
            Some(stats) = stats_rx.recv() => {
                for s in stats {
                    block_stats_cache.insert(s.height, s);
                }
                rpc_spinner = rpc_spinner.wrapping_add(1);
                redraw = true;
            }
            Some(Ok(event)) = event_stream.next() => {
                if let Event::Key(key) = event {
                    if key.kind == KeyEventKind::Press {
                        redraw = true;
                        if show_bit_modal {
                            match key.code {
                                KeyCode::Esc | KeyCode::Enter | KeyCode::Char('q') => {
                                    show_bit_modal = false;
                                }
                                _ => {}
                            }
                        } else if show_block_modal {
                            match key.code {
                                KeyCode::Esc | KeyCode::Char('q') => {
                                    show_block_modal = false;
                                }
                                KeyCode::Down => {
                                    let max = node_data.recent_blocks.len().saturating_sub(1) as u8;
                                    selected_block = (selected_block + 1).min(max);
                                }
                                KeyCode::Up => {
                                    selected_block = selected_block.saturating_sub(1);
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
                                    if screen == Screen::Signaling {
                                        signaling_notify.notify_one();
                                    }
                                    poll_notify.notify_one();
                                }
                                KeyCode::Down => {
                                    if screen == Screen::Signaling {
                                        selected_bit = (selected_bit + 1).min(28);
                                    } else if screen == Screen::Dashboard {
                                        if blocks_focused {
                                            let max = node_data.recent_blocks.len().saturating_sub(1) as u8;
                                            selected_block = (selected_block + 1).min(max);
                                        } else {
                                            peer_scroll = peer_scroll.saturating_add(1);
                                        }
                                    } else {
                                        peer_scroll = peer_scroll.saturating_add(1);
                                    }
                                }
                                KeyCode::Up => {
                                    if screen == Screen::Signaling {
                                        selected_bit = selected_bit.saturating_sub(1);
                                    } else if screen == Screen::Dashboard {
                                        if blocks_focused {
                                            selected_block = selected_block.saturating_sub(1);
                                        } else {
                                            peer_scroll = peer_scroll.saturating_sub(1);
                                        }
                                    } else {
                                        peer_scroll = peer_scroll.saturating_sub(1);
                                    }
                                }
                                KeyCode::Char('j') | KeyCode::Char('k') => {
                                    if screen == Screen::Dashboard {
                                        blocks_focused = !blocks_focused;
                                    }
                                }
                                KeyCode::Enter => {
                                    if screen == Screen::Signaling {
                                        show_bit_modal = true;
                                    } else if screen == Screen::Dashboard {
                                        let max = node_data.recent_blocks.len().saturating_sub(1) as u8;
                                        if selected_block <= max {
                                            if let Some(b) = node_data.recent_blocks.get(selected_block as usize) {
                                                if block_stats_cache.contains_key(&b.height) {
                                                    show_block_modal = true;
                                                }
                                            }
                                        }
                                    }
                                }
                                KeyCode::Char('r') => {
                                    if screen == Screen::Signaling {
                                        signaling_notify.notify_one();
                                    } else {
                                        force_full_fetch.store(true, Ordering::Relaxed);
                                        poll_notify.notify_one();
                                    }
                                }
                                KeyCode::Char('d') => {
                                    if screen == Screen::Dashboard {
                                        let blocks: Vec<(u64, String)> = node_data
                                            .recent_blocks
                                            .iter()
                                            .filter(|b| b.height > 0 && !block_stats_cache.contains_key(&b.height))
                                            .map(|b| (b.height, b.hash.clone()))
                                            .collect();
                                        if !blocks.is_empty() {
                                            let c = stats_client.clone();
                                            let tx = stats_tx.clone();
                                            tokio::spawn(async move {
                                                if let Ok(stats) = c.fetch_block_stats(&blocks).await {
                                                    let _ = tx.send(stats).await;
                                                }
                                            });
                                        }
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                } else if let Event::Resize(_, _) = event {
                    redraw = true;
                }
            }
        }

        if redraw {
            let draw_data = if screen == Screen::Signaling {
                &signaling_data
            } else {
                &node_data
            };
            terminal.draw(|f| ui::draw(f, draw_data, peer_scroll, screen, selected_bit, show_bit_modal, rpc_spinner, &block_stats_cache, selected_block, show_block_modal, blocks_focused))?;
        }
    }

    // Drop event stream before draining to release the internal reader
    drop(event_stream);
    terminal.clear()?;
    disable_raw_mode()?;
    stdout().execute(LeaveAlternateScreen)?;
    stdout().execute(crossterm::cursor::Show)?;
    println!();
    Ok(())
}
