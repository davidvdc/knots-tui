# knots-tui

Rust TUI dashboard for monitoring a Bitcoin Knots node via JSON-RPC.

## Build

No local Rust toolchain — build via Docker targeting x86_64 Ubuntu:

```bash
rm -rf out && docker build --platform linux/amd64 --output type=local,dest=./out -f Dockerfile .
```

Binary output: `out/knots-tui`

## Run

```bash
./knots-tui --rpc-url http://<node-ip>:8332 --cookie-file /path/to/.cookie
```

Or via env vars: `KNOTS_RPC_URL`, `KNOTS_COOKIE_FILE`. Default refresh interval is 5 seconds (`--interval`).

## Project structure

- `src/main.rs` — CLI args (clap), async event loop, terminal setup/teardown, data ingestion from channels
- `src/service.rs` — `AppService`: async operations (polling, fetching, backfill), loading/spinner state (atomics)
- `src/rpc.rs` — RPC client with batched JSON-RPC calls, all data structs (`NodeData`, `BlockInfo`, `BlockStats`, `PeerInfo`, etc.)
- `src/sys.rs` — System stats sampler (CPU, memory, disk I/O, process tracking from /proc)
- `src/ui/mod.rs` — `Screen` trait, `SharedState`, `draw()` dispatcher (header/body/footer), screen navigation
- `src/ui/dashboard.rs` — `DashboardScreen`: info cards, blocks table, peers table, block detail modal
- `src/ui/ibd.rs` — `IbdScreen`: IBD progress, system stats bars, peers table
- `src/ui/known_peers.rs` — `KnownPeersScreen`: time-bucketed addresses, services by network
- `src/ui/signaling.rs` — `SignalingScreen`: version bits, softforks, bit detail modal
- `src/ui/analytics.rs` — `AnalyticsScreen`: daily breakdown table, DayAgg aggregation
- `src/ui/charts.rs` — `ChartsScreen`: daily/hourly time-series charts
- `src/ui/common.rs` — Shared format helpers (`format_number`, `format_bytes`, etc.) + tests
- `Dockerfile` — Builds x86_64 Linux binary using `rust:latest` image

## Architecture

### Screen trait

All screens implement the `Screen` trait with parameter-free methods:

```rust
pub trait Screen {
    fn name(&self) -> &str;
    fn footer_hint(&self) -> &str;
    fn draw(&self, f: &mut Frame, area: Rect);
    fn handle_key(&mut self, key: KeyCode) -> KeyResult;
    fn has_modal(&self) -> bool;
    fn draw_modal(&self, f: &mut Frame, area: Rect);
    fn handle_modal_key(&mut self, key: KeyCode);
    fn available(&self) -> bool;
    fn on_enter(&mut self);
}
```

Screens receive dependencies at construction:
- `Arc<AppService>` — async operations, loading state, spinner
- `Rc<RefCell<SharedState>>` — node data, block stats cache, analytics data, system stats

Screens borrow state internally via `self.state.borrow()` / `self.state.borrow_mut()`.
Safe because the app uses a single-threaded tokio runtime (`current_thread`).

### AppService

`Arc<AppService>` is the service layer shared across all screens and background tasks:
- **Polling**: `start_polling()`, `stop_polling()`, `force_refresh()` — controls the dashboard poll loop
- **Data fetching**: `fetch_known_peers()`, `notify_signaling()`, `fetch_older_blocks()`, `fetch_new_block_stats()`
- **Analytics**: `spawn_backfill()`, `stop_backfill()`
- **State flags**: `set_loading()` / `is_loading()`, `is_fetching_older_blocks()`, `inc_spinner()` / `spinner()` — all atomic

### SharedState

`Rc<RefCell<SharedState>>` holds all mutable app data:
- `node_data: NodeData` — latest dashboard/known-peers data from RPC
- `signaling_data: NodeData` — latest signaling data (separate from dashboard)
- `block_stats_cache: HashMap<u64, BlockStats>` — per-block analysis results
- `analytics: AnalyticsData` — backfill progress, stats collection, depth
- `system_stats: SystemStats` — CPU, memory, disk I/O, bitcoind/tor process stats

Main loop mutates SharedState when data arrives from channels. Screens read it during draw/key handling.

### Data flow

1. **Background tokio tasks** fetch data and send via `mpsc` channels
2. **Main event loop** receives from channels via `tokio::select!`, mutates SharedState
3. **Screens render** on redraw by borrowing SharedState (immediate-mode rendering pattern)

Screens don't react to data — they're passive renderers that read current state on each frame.

### Screen lifecycle

- `on_enter()` — called on Tab switch; screen tells AppService what data to fetch and sets loading state
- `available()` — controls Tab navigation; IBD screen available only during IBD, Dashboard/Analytics/Charts only when synced
- Auto-switch: main loop switches between Dashboard ↔ IBD based on `initialblockdownload` flag

### Poll loop

The dashboard poll loop runs as a tokio task controlled by `poll_active: AtomicBool`:
- When active: quick-checks every N seconds, full fetch on height/peer changes or every 60s
- When paused: waits for notify (other screens pause polling)
- Screens control it via `svc.start_polling()` / `svc.stop_polling()` in their `on_enter()`

### Tab navigation

- `Tab` / `Shift+Tab` — forward/backward through available screens
- During IBD: IBD → Known Peers → Signaling → IBD (Dashboard/Analytics/Charts hidden)
- After sync: Dashboard → Known Peers → Signaling → Analytics → Charts → Dashboard

### Loading indicator

- `svc.set_loading(true)` in `on_enter()` — header shows "(loading...)" with yellow border
- `svc.set_loading(false)` when data arrives in main loop
- Dashboard shows "Connecting..." screen before first RPC response

## System stats & process tracking

- `SystemSampler` reads `/proc` every 1 second: CPU per-core, memory, disk I/O, process stats
- Memory uses htop-style "used" (excludes buffers/cached)
- Process detection via `find_pid_by_name()`: scans `/proc/*/comm` (starts_with match) with `/proc/*/cmdline` fallback
- **bitcoind**: detected by comm starting with "bitcoin" or containing "knots" (handles renamed binaries like `bitcoind2`)
- **tor**: detected by comm starting with "tor"; only shown in System card when node has onion peers (`.onion` in peer addr or local addresses)
- CPU% calculated from `/proc/[pid]/stat` utime+stime deltas between samples
- RSS memory from `/proc/[pid]/statm` (pages * 4096)
- Process stats hidden (not "not found") when process isn't running locally

## Block stats

- `BlockStats` cached by height in `SharedState.block_stats_cache`
- Auto-fetched for newly mined blocks (tip height increase detected)
- On restart, old blocks show `-` until user presses `d`
- `d` fetches stats for all blocks not in cache (spawns async task)
- Stats fetched via `getblockstats` (totals) + `getblock` verbosity 2 (tx classification)
- Each tx classified into exactly one mutually exclusive bucket (totals add up):
  - **Runes** — OP_RETURN starting with `6a5d` (OP_RETURN + OP_13)
  - **BRC-20** — ordinals witness envelope (`0063036f7264`) containing `6272632d3230` ("brc-20")
  - **Inscriptions** — ordinals witness envelope (excl. BRC-20), or witness item > 1040 hex chars
  - **OPNET** — 5-item witness, control block 130 hex chars, tapscript contains `026f70` ("op" magic)
  - **Stamps** — bare multisig outputs (no OP_RETURN or inscription)
  - **Counterparty** — OP_RETURN containing `434e545250525459` ("CNTRPRTY")
  - **Omni** — OP_RETURN containing `6f6d6e69` ("omni")
  - **OP_RETURN other** — unclassified nulldata
  - **Financial** — none of the above
- Priority order for classification: BRC-20 > Inscription > OPNET > Rune > Counterparty > Omni > Stamp > OP_RETURN other > Financial
- Taproot usage tracked non-exclusively (spending from / creating to)
- Tracks oversized OP_RETURNs (>83 bytes, the Core pre-v29 / Knots limit)
- Per-protocol vsize tracked for size% columns (total_vsize, financial_vsize, rune_vsize, etc.)
- Stats persisted to `~/.knots-tui/blockstats.jsonl` (one JSON line per block, append-only)
- Incomplete entries (missing vsize) are purged and re-fetched on next analysis

## Screens

### Dashboard
- 4 info cards — fixed 9 rows:
  - **Blockchain**: height, headers, sync status, difficulty, hashrate, disk (with pruned indicator)
  - **Mempool**: txs, size, memory, fees, relay/min fee, loaded status
  - **Network**: connections, protocol, recv/sent totals, relay/incremental fee, local addresses
  - **System**: CPU avg%, memory (htop-style used, excludes buffers/cached), disk I/O totals, bitcoind process CPU%/RSS, tor process CPU%/RSS (shown only when node has onion peers)
- System stats update every 1 second
- Warnings shown via `F1` popup modal (footer hint only appears when warnings exist)
- Recent blocks table (last 8) — fixed 11 rows
  - Always shows: Height, TXs, Size, Weight, Age, BIP110, BTC Out, Fees, Fin%, >83B
  - Financial columns show `-` when stats not loaded
  - `j/k` toggles focus between blocks and peers tables (yellow border = focused)
  - `↑/↓` navigates the focused table
  - `>` marker on selected block row when blocks table is focused
  - `Enter` opens block detail modal (if stats loaded)
  - Block detail modal: navigable with `↑/↓`, shows financial/data breakdown, protocol breakdown, taproot usage, OP_RETURN size analysis
- Peers table — fills remaining space

### IBD (Initial Block Download)
- Shown automatically when node is syncing (replaces Dashboard in Tab rotation)
- Progress bar, sync speed, ETA, download rate
- System stats: CPU bars, memory/swap bars, disk I/O rates
- Peers table (shared component from dashboard)
- Auto-switches to Dashboard when IBD completes

### Known Peers
- Time-bucketed address table by network type
- Services by network table with adoption %, node's own flags marked with `*`
- Service flags parsed from `localservices` hex in `getnetworkinfo`

### Signaling
- Version bits table: all 29 BIP9 bits (0–28), with selection cursor and Enter for detail modal
- Softforks table: merged hardcoded buried forks + node-reported forks, sorted newest first
- Known BIP9 bit assignments: 0=csv, 1=segwit, 2=taproot, 4=reduced_data(BIP110)
- Bits 13–28 are BIP320 nonce rolling (ASICBoost), shown in dark grey

### Analytics
- Daily breakdown table: blocks, txs, fin%, fin size%, data%, data size%, per-protocol count + % of data
- Protocols shown: Runes, Inscriptions, BRC-20, OPNET, Stamps, OP_RETURN other
- `s` starts/resumes analysis (~30 days, ~4320 blocks, recent-to-old for pruned node compat)
- `Esc` stops running analysis (partial results kept)
- Loads jsonl on tab entry, detects gaps, shows missing count in title
- Newly mined blocks (auto-fetched on dashboard) added to analytics history and jsonl
- Overview columns (Fin/Data) in green/yellow; detail columns in LightMagenta
- `|` separator between overview and protocol detail columns
- Nominal numbers use compact format (`format_compact`: 1.4k, 52k, 1.2m), right-aligned

## Conventions

- Do not add Co-Authored-By lines to commit messages
- Use single-line commit messages (`git commit -m "message"`) — no heredocs or multi-line
- Release notes: pass directly to `--notes "..."` — no heredocs (cat/EOF). Use `\n` for newlines if needed
- Keep column widths tight — match to actual data width
- Top info cards fixed at 9 rows; recent blocks at 11 rows; peers fills remaining space
- Connection type column is 19 chars wide (longest value: `outbound-full-relay`)
- Footer shows ASCII dot spinner `[.  ]` that advances on each RPC call (including quick-checks via `spinner_notify`)
- Use `↑/↓` in footer hints (not j/k) for non-dashboard tabs
- Never delete releases — once published, a release is permanent
- Use minor version bumps (e.g. 0.10.0 → 0.10.1) for small iterations/fixes within the same feature set
- Use major version bumps (e.g. 0.10.x → 0.11.0) for new features or significant changes
