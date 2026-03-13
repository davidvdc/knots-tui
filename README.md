# knots-tui

A terminal dashboard for monitoring your Bitcoin Knots node in real time.

```
╭──────────────────────────────────────────────────────────────────────╮
│  Bitcoin Knots Dashboard | /Satoshi:27.1.0/ | chain: main | uptime  │
╰──────────────────────────────────────────────────────────────────────╯
╭ Blockchain ──╮╭ Mempool ─────╮╭ Network ─────╮╭ Mining ──────╮
│ Height  ...  ││ TXs     ...  ││ Conns   ...  ││ Hashrate ... │
│ Headers ...  ││ Size    ...  ││ Protocol ... ││ Pooled   ... │
│ Synced  ...  ││ Memory  ...  ││ Recv    ...  │╰──────────────╯
│ Diff    ...  ││ Fees    ...  ││ Sent    ...  │
╰──────────────╯╰──────────────╯╰──────────────╯
╭ Recent Blocks ───────────────────────────────────────────────────────╮
│ Height    TXs    Size      Weight       Age                         │
│ 890,123   4,231  1.54 MB   3998.1 kvWU  12m                        │
╰──────────────────────────────────────────────────────────────────────╯
╭ Peers (42) | known: 18,392 ─────────────────────────────────────────╮
│ ID  Address          Client       Type                 TX  Dir  ... │
╰─────────────────────────────────────────────────────────────────────╯
```

## Features

- **Blockchain** — block height, headers, sync progress, difficulty, disk usage, pruning and IBD status
- **Mempool** — transaction count, size, memory usage, total fees, min fee rates
- **Network** — connections (in/out), protocol version, total traffic, relay fees, local addresses
- **Mining / Warnings** — network hashrate, pooled transactions, node warnings
- **Recent Blocks** — last 8 blocks with height, tx count, size, weight, and age
- **Peers** — full table with client user-agent, connection type, tx relay, direction, synced height, ping, connection duration, traffic
- **Known peers** — count of addresses in the node's peer database

Data refreshes every 5 seconds (configurable) using batched RPC calls for efficiency.

## Install

Download the pre-built x86_64 Linux binary:

```bash
curl -L -o knots-tui "https://github.com/davidvdc/knots-tui/raw/main/out/knots-tui" && chmod +x knots-tui
```

## Build from source

Requires Docker (no local Rust toolchain needed):

```bash
git clone https://github.com/davidvdc/knots-tui.git
cd knots-tui
docker build --platform linux/amd64 --output type=local,dest=./out -f Dockerfile .
```

Binary will be at `out/knots-tui`.

## Usage

```bash
./knots-tui --rpc-url http://<node-ip>:8332 --cookie-file /path/to/.cookie
```

### Options

| Flag | Env var | Default | Description |
|---|---|---|---|
| `--rpc-url` | `KNOTS_RPC_URL` | `http://127.0.0.1:8332` | Bitcoin Knots RPC endpoint |
| `--cookie-file` | `KNOTS_COOKIE_FILE` | `~/.bitcoin/.cookie` | Path to the `.cookie` auth file |
| `--interval` | | `5` | Refresh interval in seconds |

### Authentication

Uses cookie-based authentication. Bitcoin Knots writes a `.cookie` file (format: `__cookie__:<token>`) to its data directory on startup. Point `--cookie-file` to it.

### Keys

| Key | Action |
|---|---|
| `q` / `Esc` | Quit |
| `j` / `k` | Scroll peers table |
| `J` / `K` | Scroll blocks table |
