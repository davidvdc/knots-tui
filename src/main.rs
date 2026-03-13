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
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

use rpc::{NodeData, RpcClient};

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

    // Channel for node data updates
    let (tx, mut rx) = mpsc::channel::<NodeData>(2);

    // Spawn background poller
    let poll_client = client.clone();
    let interval = Duration::from_secs(args.interval);
    tokio::spawn(async move {
        loop {
            match poll_client.fetch_all().await {
                Ok(data) => {
                    let _ = tx.send(data).await;
                }
                Err(e) => {
                    // Send error state
                    let _ = tx
                        .send(NodeData {
                            error: Some(format!("{}", e)),
                            ..Default::default()
                        })
                        .await;
                }
            }
            tokio::time::sleep(interval).await;
        }
    });

    // TUI setup
    enable_raw_mode()?;
    stdout().execute(EnterAlternateScreen)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout()))?;
    terminal.clear()?;

    let mut node_data = NodeData::default();
    let mut last_render = Instant::now();
    let mut peer_scroll: u16 = 0;
    let mut block_scroll: u16 = 0;

    loop {
        // Drain latest data from channel
        while let Ok(data) = rx.try_recv() {
            node_data = data;
        }

        if last_render.elapsed() >= Duration::from_millis(100) {
            terminal.draw(|f| ui::draw(f, &node_data, peer_scroll, block_scroll))?;
            last_render = Instant::now();
        }

        if event::poll(Duration::from_millis(50))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    match key.code {
                        KeyCode::Char('q') | KeyCode::Esc => break,
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
    terminal.clear()?;
    Ok(())
}
