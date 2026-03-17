# knots-tui

A terminal dashboard for monitoring your Bitcoin Knots node in real time.

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

## Tabs

### Dashboard (quick-check every 5s, full refresh on changes or every 60s)

- **Blockchain** — block height, headers, sync progress, difficulty, disk usage, pruning and IBD status
- **Mempool** — transaction count, size, memory usage, total fees, min fee rates
- **Network** — connections (in/out), protocol version, total traffic, relay fees, local addresses
- **Mining / Warnings** — network hashrate, pooled transactions, node warnings
- **Recent Blocks** — last 8 blocks with height, tx count, size, weight, age, BIP110 signaling, BTC output, fees, financial tx %, and oversized OP_RETURN count
  - Financial columns always visible (`-` when stats not loaded)
  - New blocks are analyzed automatically when mined
  - Press `d` to load stats for older blocks on restart
  - Press `Enter` on a block to open a detail modal with full breakdown
- **Peers** — full table with client user-agent, connection type, tx relay, direction, synced height, ping, connection duration, traffic

### Known Peers (refresh with `r`)

- **Addresses by Last Seen** — time-bucketed breakdown by network type (ipv4, ipv6, onion, i2p, cjdns)
- **Services by Network** — node service flags (NODE_NETWORK, NODE_WITNESS, NODE_COMPACT_FILTERS, etc.) with adoption percentages per network, your node's flags marked with `*`

### Signaling (auto-loads on tab entry, refresh with `r`)

- **Version Bit Signaling** — all 29 BIP9 version bits (0–28) from the last 2,016 blocks (~1 retarget period), with signal count and percentage. Known deployments (csv, segwit, taproot, reduced_data) labeled. BIP320 nonce rolling bits (13–28) shown in grey. Select a bit and press Enter for a detailed explanation modal.
- **Softforks** — all known soft forks including buried deployments (bip34, bip66, bip65, csv, segwit, taproot) with activation heights, and any active BIP9 deployments with signaling progress

### Analytics (press `s` to start, `Esc` to stop)

- **Daily Breakdown** — table with daily aggregated transaction analysis over ~30 days (~4320 blocks)
  - Per day: block count, transaction count, financial %, financial size %, data/spam %, data size %
  - Per-protocol detail (% of data): Runes, Inscriptions, BRC-20, OPNET, Stamps, OP_RETURN other
  - Fetches recent-to-old (pruned node friendly)
  - Existing data loaded automatically on tab entry from `~/.knots-tui/blockstats.jsonl`
  - Missing blocks detected and shown in title — press `s` to fill gaps
  - Newly mined blocks automatically added to history
  - `Esc` stops a running analysis early (partial results shown)

Data fetched using batched RPC calls for efficiency. Dashboard uses cheap quick-checks (`getblockcount` + `getconnectioncount`) every 5 seconds, only triggering a full RPC fetch when block height or peer count changes, or every 60 seconds. Event-driven rendering (zero CPU when idle).

## Block Detail Modal

Press `Enter` on a selected block (with stats loaded) to see:

- Total BTC output and fees
- Transaction breakdown: financial vs data/spam (mutually exclusive — totals add up)
- **Protocol breakdown** with per-protocol tx count and percentage:
  - **Runes** — OP_RETURN with OP_13 tag (`6a5d`)
  - **BRC-20** — ordinals inscription containing `"brc-20"` JSON payload
  - **Inscriptions** — ordinals witness envelope (excl. BRC-20)
  - **OPNET** — tapscript with `"op"` magic bytes (5-item witness, 65-byte control block)
  - **Stamps** — bare multisig outputs (SRC-20/Stamps protocol)
  - **Counterparty** — OP_RETURN with `CNTRPRTY` prefix
  - **Omni Layer** — OP_RETURN with `omni` prefix
  - **OP_RETURN other** — unclassified nulldata outputs
  - **Other** — data transactions not matching any known protocol
- **Taproot usage** — transactions spending from and creating taproot outputs
- **OP_RETURN size analysis** — oversized count (>83 bytes), largest OP_RETURN size

Navigate between blocks with `↑/↓` while the modal is open.

## Install

Download the pre-built x86_64 Linux binary from [releases](https://github.com/davidvdc/knots-tui/releases):

```bash
wget -O knots-tui https://github.com/davidvdc/knots-tui/releases/latest/download/knots-tui && chmod +x knots-tui
```

## Usage

All parameters are optional and have sensible defaults:

```bash
./knots-tui [--rpc-url <url>] [--cookie-file <path>] [--interval <seconds>]
```

| Flag | Env var | Default | Description |
|---|---|---|---|
| `--rpc-url` | `KNOTS_RPC_URL` | `http://127.0.0.1:8332` | Bitcoin Knots RPC endpoint |
| `--cookie-file` | `KNOTS_COOKIE_FILE` | `~/.bitcoin/.cookie` | Path to the `.cookie` auth file |
| `--interval` | | `5` | Refresh interval in seconds |

### Authentication

Uses cookie-based authentication. Bitcoin Knots writes a `.cookie` file (format: `__cookie__:<token>`) to its data directory on startup. Point `--cookie-file` to it.

### Keys

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
