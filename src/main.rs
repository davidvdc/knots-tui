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
use std::collections::{HashMap, HashSet};
use std::io::stdout;
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU16, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, Notify};

use rpc::{BlockStats, NodeData, RpcClient};

fn stats_file_path() -> std::path::PathBuf {
    let dir = shellexpand::tilde("~/.knots-tui").to_string();
    let path = std::path::PathBuf::from(&dir);
    let _ = std::fs::create_dir_all(&path);
    path.join("blockstats.jsonl")
}

fn append_stats_to_file(stat: &BlockStats) {
    use std::io::Write;
    let path = stats_file_path();
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
        if let Ok(line) = serde_json::to_string(stat) {
            let _ = writeln!(f, "{}", line);
        }
    }
}

fn load_stats_from_file() -> Vec<BlockStats> {
    let path = stats_file_path();
    let mut stats = Vec::new();
    if let Ok(content) = std::fs::read_to_string(&path) {
        for line in content.lines() {
            if let Ok(s) = serde_json::from_str::<BlockStats>(line) {
                stats.push(s);
            }
        }
    }
    stats
}

fn rewrite_stats_file(stats: &[BlockStats]) {
    use std::io::Write;
    let path = stats_file_path();
    if let Ok(mut f) = std::fs::File::create(&path) {
        for s in stats {
            if let Ok(line) = serde_json::to_string(s) {
                let _ = writeln!(f, "{}", line);
            }
        }
    }
}

/// State for the analytics/block stats background task
#[derive(Clone, PartialEq)]
pub enum AnalyticsState {
    Idle,           // no data yet (waiting for first dashboard fetch)
    Running,        // backfill in progress
    Done,           // analysis complete (or stopped)
}

pub struct AnalyticsData {
    pub state: AnalyticsState,
    pub stats: Vec<BlockStats>,       // all loaded stats, sorted by height
    pub progress_current: u64,        // blocks analyzed so far
    pub progress_total: u64,          // total blocks to analyze
    pub missing_blocks: u64,          // gaps in the dataset
    pub scroll: u16,                  // scroll offset for analytics table
    pub depth: u64,                   // how many blocks back from tip (grows by 4320 on '+')
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Screen {
    Dashboard = 0,
    KnownPeers = 1,
    Signaling = 2,
    Analytics = 3,
}

impl Screen {
    fn from_u8(v: u8) -> Self {
        match v {
            1 => Screen::KnownPeers,
            2 => Screen::Signaling,
            3 => Screen::Analytics,
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

/// Spawn the unified block stats backfill task.
/// Fetches recent blocks (by hash) first, then analytics backfill (by height, recent-to-old).
fn spawn_backfill(
    client: &RpcClient,
    stats_tx: &mpsc::Sender<BlockStats>,
    stop: &Arc<AtomicBool>,
    recent_blocks: &[(u64, String)],
    analytics_heights: Vec<u64>,
    cached: &HashSet<u64>,
) -> u64 {
    // Recent blocks not yet cached (fetch by hash for efficiency)
    let recent: Vec<(u64, String)> = recent_blocks
        .iter()
        .filter(|(h, _)| !cached.contains(h))
        .cloned()
        .collect();
    // Analytics heights not yet cached and not in recent set
    let recent_heights: HashSet<u64> = recent.iter().map(|(h, _)| *h).collect();
    let backfill: Vec<u64> = analytics_heights
        .into_iter()
        .filter(|h| !cached.contains(h) && !recent_heights.contains(h))
        .collect();

    let total = (recent.len() + backfill.len()) as u64;
    if total == 0 {
        return 0;
    }

    let c = client.clone();
    let tx = stats_tx.clone();
    let stop = stop.clone();
    tokio::spawn(async move {
        // Phase 1: recent blocks (have hashes, one batch call each)
        for (height, hash) in recent {
            if stop.load(Ordering::Relaxed) { break; }
            if let Ok(stats) = c.fetch_block_stats(&[(height, hash)]).await {
                for s in stats {
                    let _ = tx.send(s).await;
                }
            }
        }
        // Phase 2: analytics backfill (by height, recent-to-old)
        for height in backfill {
            if stop.load(Ordering::Relaxed) { break; }
            if let Ok(stat) = c.fetch_block_stats_by_height(height).await {
                let _ = tx.send(stat).await;
            }
        }
    });

    total
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
                Screen::Signaling | Screen::Analytics => None, // handled by their own tasks
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

    // Unified block stats channel — serves both dashboard details and analytics
    let (stats_tx, mut stats_rx) = mpsc::channel::<BlockStats>(16);
    let backfill_stop = Arc::new(AtomicBool::new(false));

    let mut node_data = NodeData::default();
    let mut signaling_data = NodeData::default();
    let mut block_stats_cache: HashMap<u64, BlockStats> = HashMap::new();
    let mut last_tip_height: u64 = 0;
    let mut peer_scroll: u16 = 0;
    let mut selected_block: u16 = 0;
    let mut block_scroll: u16 = 0;
    let mut show_block_modal = false;
    let mut blocks_focused = true; // true = blocks table, false = peers table
    let mut screen = Screen::Dashboard;
    let mut selected_bit: u8 = 0;
    let mut show_bit_modal = false;
    let mut rpc_spinner: u8 = 0;
    let mut analytics = AnalyticsData {
        state: AnalyticsState::Idle,
        stats: Vec::new(),
        progress_current: 0,
        progress_total: 0,
        missing_blocks: 0,
        scroll: 0,
        depth: 4320,
    };
    let mut backfill_started = false;
    let mut prev_ibd_height: u64 = 0;
    let mut prev_ibd_bytes_recv: u64 = 0;
    let mut prev_ibd_fetched_at: u64 = 0;

    let mut event_stream = EventStream::new();

    // Initial render
    terminal.draw(|f| ui::draw(f, &node_data, peer_scroll, screen, selected_bit, show_bit_modal, rpc_spinner, &block_stats_cache, selected_block, block_scroll, show_block_modal, blocks_focused, &analytics))?;

    loop {
        let mut redraw = false;

        // Wait for: channel data, block stats, or keyboard event
        tokio::select! {
            Some(mut data) = rx.recv() => {
                if !data.recent_block_versions.is_empty() {
                    signaling_data = data.clone();
                }
                if data.error.is_some() {
                    node_data.error = data.error;
                } else if !data.recent_blocks.is_empty() {
                    // Dashboard fetch — full data
                    let new_tip = data.recent_blocks.first().map(|b| b.height).unwrap_or(0);

                    if !backfill_started {
                        // First dashboard data: load jsonl, seed cache + analytics,
                        // then start unified backfill for recent blocks + analytics range
                        backfill_started = true;
                        let mut loaded = load_stats_from_file();
                        let had_incomplete = loaded.iter().any(|s| s.total_vsize == 0);
                        loaded.retain(|s| s.total_vsize > 0);
                        if had_incomplete {
                            rewrite_stats_file(&loaded);
                        }
                        for s in &loaded {
                            block_stats_cache.insert(s.height, s.clone());
                        }
                        analytics.stats = loaded;
                        analytics.stats.sort_by_key(|s| s.height);

                        let cached: HashSet<u64> = block_stats_cache.keys().copied().collect();
                        let start = new_tip.saturating_sub(analytics.depth);
                        let analytics_heights: Vec<u64> = (start..=new_tip).rev().collect();
                        let recent: Vec<(u64, String)> = data.recent_blocks
                            .iter()
                            .map(|b| (b.height, b.hash.clone()))
                            .collect();

                        backfill_stop.store(false, Ordering::Relaxed);
                        let total = spawn_backfill(
                            &client, &stats_tx, &backfill_stop,
                            &recent, analytics_heights, &cached,
                        );
                        if total > 0 {
                            analytics.state = AnalyticsState::Running;
                            analytics.progress_current = 0;
                            analytics.progress_total = total;
                            let gaps = total;
                            analytics.missing_blocks = gaps;
                        } else {
                            analytics.state = AnalyticsState::Done;
                            analytics.missing_blocks = 0;
                        }
                    } else if new_tip > last_tip_height && last_tip_height > 0 {
                        // New blocks mined — fetch their stats
                        let new_blocks: Vec<(u64, String)> = data
                            .recent_blocks
                            .iter()
                            .filter(|b| b.height > last_tip_height && !block_stats_cache.contains_key(&b.height))
                            .map(|b| (b.height, b.hash.clone()))
                            .collect();
                        if !new_blocks.is_empty() {
                            let c = client.clone();
                            let tx = stats_tx.clone();
                            tokio::spawn(async move {
                                for (height, hash) in new_blocks {
                                    if let Ok(stats) = c.fetch_block_stats(&[(height, hash)]).await {
                                        for s in stats {
                                            let _ = tx.send(s).await;
                                        }
                                    }
                                }
                            });
                        }
                    }
                    last_tip_height = new_tip;
                    // Compute IBD sync speed and download rate from consecutive fetches
                    if prev_ibd_fetched_at > 0 && data.fetched_at > prev_ibd_fetched_at {
                        let dt = (data.fetched_at - prev_ibd_fetched_at) as f64;
                        data.ibd_blocks_per_sec = data.blockchain.blocks.saturating_sub(prev_ibd_height) as f64 / dt;
                        data.ibd_recv_per_sec = (data.net_totals.totalbytesrecv.saturating_sub(prev_ibd_bytes_recv) as f64 / dt) as u64;
                    }
                    prev_ibd_height = data.blockchain.blocks;
                    prev_ibd_bytes_recv = data.net_totals.totalbytesrecv;
                    prev_ibd_fetched_at = data.fetched_at;
                    node_data = data;
                } else if !data.known_addresses.is_empty() {
                    // KnownPeers fetch — merge without clobbering dashboard fields
                    node_data.blockchain = data.blockchain;
                    node_data.network = data.network;
                    node_data.uptime = data.uptime;
                    node_data.fetched_at = data.fetched_at;
                    node_data.known_peers = data.known_peers;
                    node_data.known_addresses = data.known_addresses;
                }
                rpc_spinner = rpc_spinner.wrapping_add(1);
                redraw = true;
            }
            _ = spinner_notify.notified() => {
                rpc_spinner = rpc_spinner.wrapping_add(1);
                redraw = true;
            }
            Some(stat) = stats_rx.recv() => {
                append_stats_to_file(&stat);
                block_stats_cache.insert(stat.height, stat.clone());
                // Add to analytics
                if !analytics.stats.iter().any(|e| e.height == stat.height) {
                    analytics.stats.push(stat);
                }
                if analytics.state == AnalyticsState::Running {
                    analytics.progress_current += 1;
                    analytics.missing_blocks = analytics.missing_blocks.saturating_sub(1);
                    if analytics.progress_current >= analytics.progress_total {
                        analytics.state = AnalyticsState::Done;
                        analytics.stats.sort_by_key(|s| s.height);
                    }
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
                                    let max = node_data.recent_blocks.len().saturating_sub(1) as u16;
                                    selected_block = (selected_block + 1).min(max);
                                    if selected_block >= block_scroll + 8 {
                                        block_scroll = selected_block - 7;
                                    }
                                }
                                KeyCode::Up => {
                                    selected_block = selected_block.saturating_sub(1);
                                    if selected_block < block_scroll {
                                        block_scroll = selected_block;
                                    }
                                }
                                _ => {}
                            }
                        } else {
                            match key.code {
                                KeyCode::Char('q') => break,
                                KeyCode::Esc => {
                                    if screen == Screen::Analytics && analytics.state == AnalyticsState::Running {
                                        backfill_stop.store(true, Ordering::Relaxed);
                                        analytics.state = AnalyticsState::Done;
                                        analytics.stats.sort_by_key(|s| s.height);
                                    } else {
                                        break;
                                    }
                                }
                                KeyCode::Tab => {
                                    screen = match screen {
                                        Screen::Dashboard => Screen::KnownPeers,
                                        Screen::KnownPeers => Screen::Signaling,
                                        Screen::Signaling => Screen::Analytics,
                                        Screen::Analytics => Screen::Dashboard,
                                    };
                                    current_screen.store(screen as u8, Ordering::Relaxed);
                                    if screen == Screen::Signaling {
                                        signaling_notify.notify_one();
                                    }
                                    if screen != Screen::Analytics {
                                        poll_notify.notify_one();
                                    }
                                }
                                KeyCode::Down => {
                                    if screen == Screen::Signaling {
                                        selected_bit = (selected_bit + 1).min(28);
                                    } else if screen == Screen::Analytics {
                                        analytics.scroll = analytics.scroll.saturating_add(1);
                                    } else if screen == Screen::Dashboard {
                                        if blocks_focused {
                                            let max = node_data.recent_blocks.len().saturating_sub(1) as u16;
                                            selected_block = (selected_block + 1).min(max);
                                            if selected_block >= block_scroll + 8 {
                                                block_scroll = selected_block - 7;
                                            }
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
                                    } else if screen == Screen::Analytics {
                                        analytics.scroll = analytics.scroll.saturating_sub(1);
                                    } else if screen == Screen::Dashboard {
                                        if blocks_focused {
                                            selected_block = selected_block.saturating_sub(1);
                                            if selected_block < block_scroll {
                                                block_scroll = selected_block;
                                            }
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
                                        let max = node_data.recent_blocks.len().saturating_sub(1) as u16;
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
                                    } else if screen != Screen::Analytics {
                                        force_full_fetch.store(true, Ordering::Relaxed);
                                        poll_notify.notify_one();
                                    }
                                }
                                KeyCode::Char('+') | KeyCode::Char('=') => {
                                    if screen == Screen::Analytics && analytics.state != AnalyticsState::Running {
                                        // Extend analytics by another 30 days (~4320 blocks)
                                        // and resume any gaps from a previous Esc stop
                                        analytics.depth += 4320;
                                        let tip = node_data.blockchain.blocks;
                                        let start = tip.saturating_sub(analytics.depth);
                                        let all_heights: Vec<u64> = (start..=tip).rev().collect();
                                        let cached: HashSet<u64> = block_stats_cache.keys().copied().collect();
                                        backfill_stop.store(false, Ordering::Relaxed);
                                        let total = spawn_backfill(
                                            &client, &stats_tx, &backfill_stop,
                                            &[], all_heights, &cached,
                                        );
                                        if total > 0 {
                                            analytics.state = AnalyticsState::Running;
                                            analytics.progress_current = 0;
                                            analytics.progress_total = total;
                                            analytics.missing_blocks = total;
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
            terminal.draw(|f| ui::draw(f, draw_data, peer_scroll, screen, selected_bit, show_bit_modal, rpc_spinner, &block_stats_cache, selected_block, block_scroll, show_block_modal, blocks_focused, &analytics))?;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn screen_from_u8_dashboard() {
        assert_eq!(Screen::from_u8(0), Screen::Dashboard);
    }

    #[test]
    fn screen_from_u8_known_peers() {
        assert_eq!(Screen::from_u8(1), Screen::KnownPeers);
    }

    #[test]
    fn screen_from_u8_signaling() {
        assert_eq!(Screen::from_u8(2), Screen::Signaling);
    }

    #[test]
    fn screen_from_u8_analytics() {
        assert_eq!(Screen::from_u8(3), Screen::Analytics);
    }

    #[test]
    fn screen_from_u8_invalid() {
        assert_eq!(Screen::from_u8(4), Screen::Dashboard);
        assert_eq!(Screen::from_u8(255), Screen::Dashboard);
    }
}
