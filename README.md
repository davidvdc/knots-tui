# knots-tui

A terminal and web dashboard for monitoring your Bitcoin Knots node in real time.

Two modes: a full-featured **TUI** for terminal use, and a **Web UI** for browser access (designed for Umbrel/self-hosted deployments).

```
+----------------------------------------------------------------------+
|  Bitcoin Knots Dashboard | /Satoshi:27.1.0/ | chain: main | uptime   |
+----------------------------------------------------------------------+
+-Blockchain------+-Mempool--------+-Network--------+-Mining-----------+
| Height  890,123 | TXs    12,345  | Conns 42(8/34) | Hashrate  824EH  |
| Headers 890,123 | Size  4.52 MB  | Protocol 70016 | Pooled TX   185  |
| Synced  YES     | Memory 298/300 | Recv  12.50 GB |                  |
| Diff   9.52e13  | Fees 0.423 BTC | Sent   8.21 GB |                  |
| Disk  620.15 GB | Min fee 0.00.. | Relay fee 0.0. |                  |
+-----------------+----------------+----------------+------------------+
+-Recent Blocks [Enter: detail | d: load missing]---------------------+
|   Height     TXs  Size     Weight      Age     BIP110 BTC Out  ... |
| > 890,123  4,231  1.54 MB  3998 kvWU   12m     yes    5221 BTC ... |
|   890,122  3,892  1.48 MB  3991 kvWU   22m     no     -        ... |
+---------------------------------------------------------------------+
+-Peers (42) | known: 18,392------------------------------------------+
| ID  Address          Client              Type                TX  Dir|
| 1   1.2.3.4:8333     Satoshi:27.0.0      outbound-full-relay yes out|
+---------------------------------------------------------------------+
 q: quit | Tab: switch screen | j/k: switch table | ↑/↓: navigate [. ]
```

## Install

Download the pre-built x86_64 Linux binary from [releases](https://github.com/davidvdc/knots-tui/releases):

```bash
wget -O knots-tui https://github.com/davidvdc/knots-tui/releases/latest/download/knots-tui && chmod +x knots-tui
```

## Usage

### TUI mode (terminal)

```bash
./knots-tui --rpc-url http://<node-ip>:8332 --cookie-file /path/to/.cookie
```

### Web mode (browser)

```bash
./knots-tui --web-port 3000 --rpc-url http://<node-ip>:8332 --rpc-user user --rpc-password pass
```

Then open `http://localhost:3000` in your browser.

### Demo mode (no node required)

```bash
./knots-tui --demo
```

Starts the web UI on port 3000 with realistic synthetic data — useful for testing and evaluation.

### Parameters

| Flag | Env var | Default | Description |
|---|---|---|---|
| `--rpc-url` | `KNOTS_RPC_URL` | `http://127.0.0.1:8332` | Bitcoin Knots RPC endpoint |
| `--cookie-file` | `KNOTS_COOKIE_FILE` | `~/.bitcoin/.cookie` | Path to `.cookie` auth file |
| `--rpc-user` | `KNOTS_RPC_USER` | | RPC username (alternative to cookie) |
| `--rpc-password` | `KNOTS_RPC_PASSWORD` | | RPC password (alternative to cookie) |
| `--interval` | | `5` | Refresh interval in seconds |
| `--web-port` | `KNOTS_WEB_PORT` | | Enable web mode on this port |
| `--demo` | | | Start web UI with synthetic demo data |

### Authentication

Two authentication methods:

- **Cookie file** (default): Bitcoin Knots writes `__cookie__:<token>` to its data directory on startup. Point `--cookie-file` to it.
- **User/password**: Set `--rpc-user` and `--rpc-password` (or env vars). Used when both are provided; overrides cookie auth.

## Tabs

Both TUI and Web UI share the same screens:

### Dashboard

- **Blockchain** — block height, headers, sync progress, difficulty, hashrate, disk usage, pruning status
- **Mempool** — transaction count, size, memory usage, total fees, min fee rates
- **Network** — connections (in/out), protocol version, total traffic, relay fees, local addresses
- **System** — CPU, memory, swap, disk I/O for system and per-process (bitcoind, tor)
- **Recent Blocks** — last 8 blocks with height, tx count, size, weight, age, BIP110 signaling, BTC output, fees, financial tx %, BIP-110 violations
  - Click/Enter on a block to open a detail modal with full protocol breakdown
- **Analytics summary** — 24h aggregation of protocol mix and BIP-110 compliance
- **Peers** — full table with client, connection type, tx relay, direction, height, ping, traffic

### IBD (Initial Block Download)

- Shown automatically when node is syncing (replaces Dashboard)
- Progress bar, sync speed, ETA, download rate
- System stats: CPU bars, memory/swap bars, disk I/O
- Peers table

### Known Peers

- **Addresses by Last Seen** — time-bucketed breakdown by network (ipv4, ipv6, onion, i2p, cjdns)
- **Services by Network** — service flags with adoption % per network, your node's flags marked with `*`

### Signaling

- **Version Bit Signaling** — all 29 BIP9 bits from last 2,016 blocks with signal count and %. Click/Enter for detail modal.
- **Softforks** — buried + BIP9 deployments with activation heights and signaling progress

### Analytics

- **Daily Breakdown** — daily aggregated transaction analysis over ~30 days
  - Financial %, data/spam %, per-protocol detail: Runes, Inscriptions, BRC-20, OPNET, Stamps, OP_RETURN other
  - BIP-110 violations and disk savings
  - Data persisted to `~/.knots-tui/blockstats.jsonl`

### Charts (web only uses Chart.js; TUI uses braille charts)

- Three modes: **OPNET**, **Data**, **BIP-110**
- Daily chart + 24h rolling window hourly chart

## Block Detail Modal

Press Enter (TUI) or click (Web) on a block with stats loaded to see:

- Total BTC output, fees, transaction count
- Protocol breakdown: financial vs data protocols (Runes, BRC-20, Inscriptions, OPNET, Stamps, Counterparty, Omni, OP_RETURN other)
- BIP-110 compliance: per-protocol per-rule (R1-R7) violation matrix
- Taproot usage stats, max observed sizes (OP_RETURN, scriptPubKey, witness element)

## Web UI

The web UI is a single HTML page embedded in the binary — no external files needed. It uses vanilla JS with Chart.js (loaded from CDN) for charts.

API endpoints:
- `GET /api/dashboard` — node data, block stats, system stats
- `GET /api/signaling` — version bits and softforks
- `GET /api/analytics` — full block stats history

## TUI Keys

| Key | Action | Screen |
|---|---|---|
| `q` / `Esc` | Quit (or close modal / stop analysis) | All |
| `Tab` | Switch between screens | All |
| `j` / `k` | Switch focus between blocks and peers tables | Dashboard |
| `↑` / `↓` | Navigate focused table / select bit | Dashboard, Known Peers, Signaling |
| `Enter` | Open block detail / bit detail modal | Dashboard, Signaling |
| `d` | Load block stats (BTC out, fees, financial %) | Dashboard |
| `s` | Start / resume block analysis | Analytics |
| `r` | Force full refresh | Dashboard, Known Peers, Signaling |
