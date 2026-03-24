mod rpc;
mod service;
mod sys;
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
use std::sync::atomic::{AtomicBool, AtomicU16, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, Notify};

use rpc::{BlockStats, NodeData, RpcClient};
use service::AppService;
use sys::SystemStats;
use ui::{SharedState, Screen as ScreenTrait};

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

#[derive(Clone, PartialEq)]
pub enum AnalyticsState {
    Idle,
    Running,
    Done,
}

pub struct AnalyticsData {
    pub state: AnalyticsState,
    pub stats: Vec<BlockStats>,
    pub progress_current: u64,
    pub progress_total: u64,
    pub missing_blocks: u64,
    pub depth: u64,
}

const SCREEN_DASHBOARD: usize = 0;
const SCREEN_IBD: usize = 5;

#[derive(Parser)]
#[command(name = "knots-tui", about = "Bitcoin Knots node dashboard")]
struct Args {
    #[arg(long, env = "KNOTS_RPC_URL", default_value = "http://127.0.0.1:8332")]
    rpc_url: String,
    #[arg(long, env = "KNOTS_COOKIE_FILE", default_value = "~/.bitcoin/.cookie")]
    cookie_file: String,
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
    let poll_notify = Arc::new(Notify::new());
    let poll_active = Arc::new(AtomicBool::new(true));
    let signaling_notify = Arc::new(Notify::new());
    let signaling_progress = Arc::new(AtomicU16::new(0));
    let force_full_fetch = Arc::new(AtomicBool::new(false));
    let spinner_notify = Arc::new(Notify::new());

    // Dashboard poll loop — only polls when poll_active is true
    let poll_client = client.clone();
    let poll_wake = poll_notify.clone();
    let poll_is_active = poll_active.clone();
    let poll_tx = tx.clone();
    let poll_force = force_full_fetch.clone();
    let poll_spinner = spinner_notify.clone();
    let quick_interval = Duration::from_secs(args.interval);
    let full_interval = Duration::from_secs(60);
    tokio::spawn(async move {
        let mut last_height: u64 = 0;
        let mut last_conns: u64 = 0;
        let mut last_full_fetch = tokio::time::Instant::now();
        let mut force_full = true;
        loop {
            if !poll_is_active.load(Ordering::Relaxed) {
                poll_wake.notified().await;
                continue;
            }
            if poll_force.swap(false, Ordering::Relaxed) { force_full = true; }
            let mut need_full = force_full;
            force_full = false;
            if !need_full {
                if let Ok((h, c)) = poll_client.fetch_tip_and_peers().await {
                    if h != last_height || c != last_conns { need_full = true; }
                }
            }
            if !need_full && last_full_fetch.elapsed() >= full_interval { need_full = true; }
            if need_full {
                let r = poll_client.fetch_dashboard().await;
                if let Ok(ref data) = r {
                    last_height = data.blockchain.blocks;
                    last_conns = data.network.connections;
                    last_full_fetch = tokio::time::Instant::now();
                }
                match r {
                    Ok(data) => { let _ = poll_tx.send(data).await; }
                    Err(e) => { let _ = poll_tx.send(NodeData { error: Some(format!("{}", e)), ..Default::default() }).await; }
                }
            } else {
                poll_spinner.notify_one();
            }
            tokio::select! {
                _ = tokio::time::sleep(quick_interval) => {}
                _ = poll_wake.notified() => {}
            }
        }
    });

    // Signaling task
    let sig_client = client.clone();
    let sig_progress = signaling_progress.clone();
    let sig_wake = signaling_notify.clone();
    let sig_tx = tx.clone();
    tokio::spawn(async move {
        loop {
            sig_wake.notified().await;
            sig_progress.store(0, Ordering::Relaxed);
            match sig_client.fetch_signaling(&sig_progress).await {
                Ok(data) => { let _ = sig_tx.send(data).await; }
                Err(e) => { let _ = sig_tx.send(NodeData { error: Some(format!("{}", e)), ..Default::default() }).await; }
            }
        }
    });

    // System stats sampler
    let (sys_tx, mut sys_rx) = mpsc::channel::<SystemStats>(2);
    tokio::spawn(async move {
        let mut sampler = sys::SystemSampler::new();
        let mut interval = tokio::time::interval(Duration::from_secs(2));
        interval.tick().await;
        loop { interval.tick().await; let _ = sys_tx.send(sampler.sample()).await; }
    });

    enable_raw_mode()?;
    stdout().execute(EnterAlternateScreen)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout()))?;
    terminal.clear()?;

    let (stats_tx, mut stats_rx) = mpsc::channel::<BlockStats>(16);
    let (older_blocks_tx, mut older_blocks_rx) = mpsc::channel::<Vec<rpc::BlockInfo>>(1);
    let backfill_stop = Arc::new(AtomicBool::new(false));

    let svc = Arc::new(AppService::new(
        client.clone(), poll_notify.clone(), poll_active.clone(),
        signaling_notify.clone(), force_full_fetch.clone(), backfill_stop.clone(),
        tx.clone(), stats_tx.clone(), older_blocks_tx.clone(),
    ));

    let mut state = SharedState {
        node_data: NodeData::default(),
        signaling_data: NodeData::default(),
        block_stats_cache: HashMap::new(),
        analytics: AnalyticsData {
            state: AnalyticsState::Idle,
            stats: Vec::new(),
            progress_current: 0,
            progress_total: 0,
            missing_blocks: 0,
            depth: 4320,
        },
        system_stats: SystemStats::default(),
        rpc_spinner: 0,
    };

    let mut screens: Vec<Box<dyn ScreenTrait>> = vec![
        Box::new(ui::dashboard::DashboardScreen::new(svc.clone())),
        Box::new(ui::known_peers::KnownPeersScreen::new(svc.clone())),
        Box::new(ui::signaling::SignalingScreen::new(svc.clone())),
        Box::new(ui::analytics::AnalyticsScreen::new(svc.clone())),
        Box::new(ui::charts::ChartsScreen::new(svc.clone())),
        Box::new(ui::ibd::IbdScreen::new(svc.clone())),
    ];
    let mut active: usize = 0;

    let mut last_tip_height: u64 = 0;
    let mut backfill_started = false;
    let mut prev_ibd_height: u64 = 0;
    let mut prev_ibd_bytes_recv: u64 = 0;
    let mut prev_ibd_fetched_at: u64 = 0;

    let mut event_stream = EventStream::new();

    terminal.draw(|f| ui::draw(f, screens[active].as_ref(), &state, &svc))?;

    loop {
        let mut redraw = false;

        tokio::select! {
            Some(mut data) = rx.recv() => {
                svc.set_loading(false);
                if !data.recent_block_versions.is_empty() {
                    state.signaling_data = data.clone();
                }
                if data.error.is_some() {
                    state.node_data.error = data.error;
                } else if !data.recent_blocks.is_empty() {
                    let new_tip = data.recent_blocks.first().map(|b| b.height).unwrap_or(0);
                    let is_ibd = data.blockchain.initialblockdownload;

                    if !backfill_started && !is_ibd {
                        backfill_started = true;
                        let mut loaded = load_stats_from_file();
                        let needs_vsize = |s: &rpc::BlockStats| s.bip110_violating_txs > 0 && s.bip110_violating_vsize == 0;
                        let had_incomplete = loaded.iter().any(|s| s.total_vsize == 0 || !s.bip110_checked || !s.bip110_per_protocol || needs_vsize(s));
                        loaded.retain(|s| s.total_vsize > 0 && s.bip110_checked && s.bip110_per_protocol && !needs_vsize(s));
                        if had_incomplete { rewrite_stats_file(&loaded); }
                        for s in &loaded { state.block_stats_cache.insert(s.height, s.clone()); }
                        state.analytics.stats = loaded;
                        state.analytics.stats.sort_by_key(|s| s.height);

                        let cached: HashSet<u64> = state.block_stats_cache.keys().copied().collect();
                        let start = new_tip.saturating_sub(state.analytics.depth);
                        let analytics_heights: Vec<u64> = (start..=new_tip).rev().collect();
                        let recent: Vec<(u64, String)> = data.recent_blocks.iter().map(|b| (b.height, b.hash.clone())).collect();

                        let total = svc.spawn_backfill(&recent, analytics_heights, &cached);
                        if total > 0 {
                            state.analytics.state = AnalyticsState::Running;
                            state.analytics.progress_current = 0;
                            state.analytics.progress_total = total;
                            state.analytics.missing_blocks = total;
                        } else {
                            state.analytics.state = AnalyticsState::Done;
                            state.analytics.missing_blocks = 0;
                        }
                    } else if new_tip > last_tip_height && last_tip_height > 0 && !is_ibd {
                        let new_blocks: Vec<(u64, String)> = data.recent_blocks.iter()
                            .filter(|b| b.height > last_tip_height && !state.block_stats_cache.contains_key(&b.height))
                            .map(|b| (b.height, b.hash.clone()))
                            .collect();
                        svc.fetch_new_block_stats(new_blocks);
                    }
                    last_tip_height = new_tip;

                    if prev_ibd_fetched_at > 0 && data.fetched_at > prev_ibd_fetched_at {
                        let dt = (data.fetched_at - prev_ibd_fetched_at) as f64;
                        data.ibd_blocks_per_sec = data.blockchain.blocks.saturating_sub(prev_ibd_height) as f64 / dt;
                        data.ibd_recv_per_sec = (data.net_totals.totalbytesrecv.saturating_sub(prev_ibd_bytes_recv) as f64 / dt) as u64;
                    }
                    prev_ibd_height = data.blockchain.blocks;
                    prev_ibd_bytes_recv = data.net_totals.totalbytesrecv;
                    prev_ibd_fetched_at = data.fetched_at;

                    let old_blocks = std::mem::take(&mut state.node_data.recent_blocks);
                    state.node_data = data;
                    let new_heights: std::collections::HashSet<u64> = state.node_data.recent_blocks.iter().map(|b| b.height).collect();
                    for b in old_blocks {
                        if !new_heights.contains(&b.height) { state.node_data.recent_blocks.push(b); }
                    }
                } else if !data.known_addresses.is_empty() {
                    state.node_data.blockchain = data.blockchain;
                    state.node_data.network = data.network;
                    state.node_data.uptime = data.uptime;
                    state.node_data.fetched_at = data.fetched_at;
                    state.node_data.known_peers = data.known_peers;
                    state.node_data.known_addresses = data.known_addresses;
                }
                state.rpc_spinner = state.rpc_spinner.wrapping_add(1);
                redraw = true;
            }
            _ = spinner_notify.notified() => {
                state.rpc_spinner = state.rpc_spinner.wrapping_add(1);
                redraw = true;
            }
            Some(stat) = stats_rx.recv() => {
                append_stats_to_file(&stat);
                state.block_stats_cache.insert(stat.height, stat.clone());
                if !state.analytics.stats.iter().any(|e| e.height == stat.height) {
                    state.analytics.stats.push(stat);
                }
                if state.analytics.state == AnalyticsState::Running {
                    state.analytics.progress_current += 1;
                    state.analytics.missing_blocks = state.analytics.missing_blocks.saturating_sub(1);
                    if state.analytics.progress_current >= state.analytics.progress_total {
                        state.analytics.state = AnalyticsState::Done;
                        state.analytics.stats.sort_by_key(|s| s.height);
                    }
                }
                state.rpc_spinner = state.rpc_spinner.wrapping_add(1);
                redraw = true;
            }
            Some(blocks) = older_blocks_rx.recv() => {
                svc.clear_fetching_older_blocks();
                let existing: std::collections::HashSet<u64> = state.node_data.recent_blocks.iter().map(|b| b.height).collect();
                for b in blocks {
                    if !existing.contains(&b.height) { state.node_data.recent_blocks.push(b); }
                }
                redraw = true;
            }
            Some(sys) = sys_rx.recv() => {
                state.system_stats = sys;
                if state.node_data.blockchain.initialblockdownload { redraw = true; }
            }
            Some(Ok(event)) = event_stream.next() => {
                if let Event::Key(key) = event {
                    if key.kind == KeyEventKind::Press {
                        redraw = true;
                        if screens[active].has_modal() {
                            screens[active].handle_modal_key(key.code, &mut state);
                        } else {
                            match key.code {
                                KeyCode::Char('q') => break,
                                KeyCode::Tab => {
                                    active = ui::next_screen(active, &screens, &state);
                                    screens[active].on_enter();
                                }
                                KeyCode::BackTab => {
                                    active = ui::prev_screen(active, &screens, &state);
                                    screens[active].on_enter();
                                }
                                _ => {
                                    let result = screens[active].handle_key(key.code, &mut state);
                                    if result == ui::KeyResult::Quit { break; }
                                }
                            }
                        }
                    }
                } else if let Event::Resize(_, _) = event {
                    redraw = true;
                }
            }
        }

        if redraw {
            let is_ibd = state.node_data.blockchain.initialblockdownload;
            if is_ibd && active == SCREEN_DASHBOARD {
                active = SCREEN_IBD;
                screens[active].on_enter();
            } else if !is_ibd && active == SCREEN_IBD {
                active = SCREEN_DASHBOARD;
                screens[active].on_enter();
            }
            terminal.draw(|f| ui::draw(f, screens[active].as_ref(), &state, &svc))?;
        }
    }

    drop(event_stream);
    terminal.clear()?;
    disable_raw_mode()?;
    stdout().execute(LeaveAlternateScreen)?;
    stdout().execute(crossterm::cursor::Show)?;
    println!();
    Ok(())
}
