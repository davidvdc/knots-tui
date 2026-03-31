use crate::rpc::{BlockStats, NodeData, RpcClient};
use crate::sys::{SystemSampler, SystemStats};
use axum::{extract::State, response::Html, routing::get, Json, Router};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU16, Ordering};
use std::sync::Arc;
use tokio::sync::RwLock;
use tower_http::cors::CorsLayer;

#[derive(Default, Clone, serde::Serialize)]
pub struct DashboardData {
    pub node: NodeData,
    pub block_stats: HashMap<u64, BlockStats>,
    pub system: SystemStats,
}

#[derive(Default, Clone, serde::Serialize)]
pub struct SignalingData {
    pub node: NodeData,
}

#[derive(Clone)]
pub struct WebState {
    pub dashboard: Arc<RwLock<DashboardData>>,
    pub signaling: Arc<RwLock<SignalingData>>,
}

fn stats_file_path() -> std::path::PathBuf {
    let dir = shellexpand::tilde("~/.knots-tui").to_string();
    std::path::PathBuf::from(&dir).join("blockstats.jsonl")
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

pub async fn run(client: RpcClient, port: u16, interval: u64) -> anyhow::Result<()> {
    let mut initial_stats: HashMap<u64, BlockStats> = HashMap::new();
    let loaded = load_stats_from_file();
    let needs_vsize = |s: &BlockStats| s.bip110_violating_txs > 0 && s.bip110_violating_vsize == 0;
    for s in loaded {
        if s.total_vsize > 0 && s.bip110_checked && s.bip110_per_protocol && !needs_vsize(&s) {
            initial_stats.insert(s.height, s);
        }
    }
    eprintln!("Loaded {} block stats from history", initial_stats.len());

    let state = WebState {
        dashboard: Arc::new(RwLock::new(DashboardData {
            block_stats: initial_stats,
            ..Default::default()
        })),
        signaling: Arc::new(RwLock::new(SignalingData::default())),
    };

    // System stats sampler
    let sys_state = state.dashboard.clone();
    tokio::spawn(async move {
        let mut sampler = SystemSampler::new();
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(1));
        interval.tick().await;
        loop {
            interval.tick().await;
            let stats = sampler.sample();
            sys_state.write().await.system = stats;
        }
    });

    // RPC polling task
    let poll_dash = state.dashboard.clone();
    let poll_client = client.clone();
    let poll_interval = std::time::Duration::from_secs(interval);
    tokio::spawn(async move {
        let mut last_height: u64 = 0;
        loop {
            match poll_client.fetch_dashboard().await {
                Ok(data) => {
                    let new_tip = data.blockchain.blocks;
                    let mut ws = poll_dash.write().await;
                    if new_tip > last_height && last_height > 0 {
                        let new_blocks: Vec<(u64, String)> = data.recent_blocks.iter()
                            .filter(|b| b.height > last_height && !ws.block_stats.contains_key(&b.height))
                            .map(|b| (b.height, b.hash.clone())).collect();
                        if !new_blocks.is_empty() {
                            let c = poll_client.clone();
                            let s = poll_dash.clone();
                            tokio::spawn(async move {
                                for (height, hash) in new_blocks {
                                    if let Ok(stats) = c.fetch_block_stats(&[(height, hash)]).await {
                                        let mut ws = s.write().await;
                                        for st in stats { ws.block_stats.insert(st.height, st); }
                                    }
                                }
                            });
                        }
                    }
                    if last_height == 0 {
                        let blocks: Vec<(u64, String)> = data.recent_blocks.iter()
                            .filter(|b| !ws.block_stats.contains_key(&b.height))
                            .map(|b| (b.height, b.hash.clone())).collect();
                        if !blocks.is_empty() {
                            let c = poll_client.clone();
                            let s = poll_dash.clone();
                            tokio::spawn(async move {
                                for (height, hash) in blocks {
                                    if let Ok(stats) = c.fetch_block_stats(&[(height, hash)]).await {
                                        let mut ws = s.write().await;
                                        for st in stats { ws.block_stats.insert(st.height, st); }
                                    }
                                }
                            });
                        }
                    }
                    last_height = new_tip;
                    ws.node = data;
                }
                Err(e) => {
                    poll_dash.write().await.node.error = Some(format!("{}", e));
                }
            }
            tokio::time::sleep(poll_interval).await;
        }
    });

    // Signaling polling task
    let sig_state = state.signaling.clone();
    let sig_client = client.clone();
    tokio::spawn(async move {
        loop {
            let progress = Arc::new(AtomicU16::new(0));
            match sig_client.fetch_signaling(&progress).await {
                Ok(data) => { sig_state.write().await.node = data; }
                Err(_) => {}
            }
            tokio::time::sleep(std::time::Duration::from_secs(120)).await;
        }
    });

    serve(state, port).await
}

pub async fn run_demo(port: u16) -> anyhow::Result<()> {
    eprintln!("Starting in demo mode with synthetic data");
    let (dashboard, signaling) = demo::generate();
    let state = WebState {
        dashboard: Arc::new(RwLock::new(dashboard)),
        signaling: Arc::new(RwLock::new(signaling)),
    };
    serve(state, port).await
}

async fn serve(state: WebState, port: u16) -> anyhow::Result<()> {
    let app = Router::new()
        .route("/", get(index_handler))
        .route("/api/dashboard", get(dashboard_handler))
        .route("/api/signaling", get(signaling_handler))
        .route("/api/analytics", get(analytics_handler))
        .layer(CorsLayer::permissive())
        .with_state(state);

    let addr = format!("0.0.0.0:{}", port);
    eprintln!("knots-tui web server listening on http://{}", addr);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

async fn index_handler() -> Html<&'static str> {
    Html(include_str!("../web/index.html"))
}

async fn dashboard_handler(State(state): State<WebState>) -> Json<DashboardData> {
    Json(state.dashboard.read().await.clone())
}

async fn signaling_handler(State(state): State<WebState>) -> Json<SignalingData> {
    Json(state.signaling.read().await.clone())
}

#[derive(serde::Serialize)]
struct AnalyticsResponse {
    stats: Vec<BlockStats>,
}

async fn analytics_handler(State(state): State<WebState>) -> Json<AnalyticsResponse> {
    let dash = state.dashboard.read().await;
    let mut stats: Vec<BlockStats> = dash.block_stats.values().cloned().collect();
    stats.sort_by_key(|s| s.height);
    Json(AnalyticsResponse { stats })
}

// ===== Demo data generator =====
mod demo {
    use super::*;
    use crate::rpc::*;
    use std::collections::BTreeMap;

    pub fn generate() -> (DashboardData, SignalingData) {
        let now = chrono::Utc::now().timestamp() as u64;
        let tip = 890_142u64;

        // --- Dashboard node data ---
        let blockchain = BlockchainInfo {
            chain: "main".into(),
            blocks: tip,
            headers: tip,
            bestblockhash: "00000000000000000002a7c4c1e48d76c5a37902165a270156b7a8d72f8804c6".into(),
            difficulty: 1.1393e14,
            time: now - 320,
            mediantime: now - 600,
            verificationprogress: 0.9999998,
            initialblockdownload: false,
            size_on_disk: 654_300_000_000,
            pruned: false,
            warnings: WarningsField::None,
            softforks: demo_softforks(),
        };

        let network = NetworkInfo {
            version: 270100,
            subversion: "/Knots:27.1.0/".into(),
            protocolversion: 70016,
            connections: 42,
            connections_in: 18,
            connections_out: 24,
            relayfee: 0.00001,
            incrementalfee: 0.00001,
            localservices: "0000000808000409".into(),
            localaddresses: vec![LocalAddress {
                address: "abcdef1234567890.onion".into(),
                port: 8333,
            }],
            warnings: WarningsField::None,
        };

        let mempool = MempoolInfo {
            loaded: true,
            size: 45_832,
            bytes: 52_400_000,
            usage: 198_000_000,
            total_fee: 1.28473,
            maxmempool: 300_000_000,
            mempoolminfee: 0.00002,
            minrelaytxfee: 0.00001,
        };

        let mining = MiningInfo {
            networkhashps: 8.42e20,
            pooledtx: 45_832,
            chain: "main".into(),
        };

        let net_totals = NetTotals {
            totalbytesrecv: 48_200_000_000,
            totalbytessent: 12_800_000_000,
        };

        let peers = demo_peers(now);
        let known_addresses = demo_known_addresses(now);
        let known_peers = known_addresses.len() as u64;
        let recent_blocks = demo_recent_blocks(tip, now);

        let node = NodeData {
            error: None,
            blockchain,
            network,
            mempool,
            mining,
            peers,
            net_totals,
            uptime: 432_000,
            recent_blocks,
            fetched_at: now,
            known_peers,
            known_addresses,
            softforks: demo_softforks(),
            ..Default::default()
        };

        let mut block_stats = HashMap::new();
        // Generate 30 days of block stats (~144 blocks/day)
        for day_offset in 0..30u64 {
            let day_base = now - (day_offset * 86400);
            let blocks_per_day = 144;
            for b in 0..blocks_per_day {
                let height = tip - (day_offset * blocks_per_day + b);
                let time = day_base - (b * 600);
                let stats = demo_block_stats(height, time, day_offset);
                block_stats.insert(height, stats);
            }
        }

        let dashboard = DashboardData {
            node,
            block_stats,
            system: SystemStats::default(),
        };

        // --- Signaling data ---
        let sig_node = demo_signaling(tip, now);
        let signaling = SignalingData { node: sig_node };

        (dashboard, signaling)
    }

    fn demo_softforks() -> BTreeMap<String, SoftFork> {
        let mut m = BTreeMap::new();
        let buried = |h: i64| SoftFork {
            fork_type: "buried".into(), active: true, height: Some(h), bip9: None,
        };
        m.insert("bip34".into(), buried(227931));
        m.insert("bip66".into(), buried(363725));
        m.insert("bip65".into(), buried(388381));
        m.insert("csv".into(), buried(419328));
        m.insert("segwit".into(), buried(481824));
        m.insert("taproot".into(), buried(709632));
        m.insert("reduced_data".into(), SoftFork {
            fork_type: "bip9".into(),
            active: false,
            height: None,
            bip9: Some(Bip9Info {
                status: "defined".into(),
                bit: Some(4),
                start_time: 1704067200,
                timeout: 1735689600,
                since: 0,
                statistics: Some(Bip9Statistics {
                    period: 2016,
                    threshold: 1815,
                    elapsed: 842,
                    count: 68,
                    possible: true,
                }),
            }),
        });
        m
    }

    fn demo_peers(now: u64) -> Vec<PeerInfo> {
        let clients = [
            "/Satoshi:27.0.0/", "/Satoshi:26.0.0/", "/Knots:27.1.0/",
            "/Satoshi:25.1.0/", "/Satoshi:28.0.0/", "/Knots:26.1.0/",
        ];
        let types = [
            "outbound-full-relay", "outbound-full-relay", "inbound",
            "outbound-block-relay", "inbound", "outbound-full-relay",
        ];
        (0..12).map(|i| {
            let inbound = i % 3 == 2;
            PeerInfo {
                id: i as u64,
                addr: if i == 4 { format!("abc{:x}def.onion:8333", i) }
                      else { format!("192.168.{}.{}:8333", i / 4 + 1, 10 + i) },
                subver: clients[i % clients.len()].into(),
                version: 70016,
                inbound,
                startingheight: 890_100 + i as i64,
                synced_headers: 890_142,
                synced_blocks: 890_142,
                pingtime: Some(0.02 + (i as f64) * 0.015),
                bytessent: 50_000_000 + i as u64 * 8_000_000,
                bytesrecv: 120_000_000 + i as u64 * 15_000_000,
                connection_type: types[i % types.len()].into(),
                conntime: now - 3600 * (i as u64 + 1),
                lastsend: now - 2,
                lastrecv: now - 1,
                relaytxes: !matches!(types[i % types.len()], "outbound-block-relay"),
            }
        }).collect()
    }

    fn demo_known_addresses(now: u64) -> Vec<KnownAddress> {
        let mut addrs = Vec::new();
        let nets = ["ipv4", "ipv6", "onion", "i2p"];
        let counts = [32000u64, 8000, 4000, 1200];
        // Service flag distributions
        let svc_templates: &[(u64, u32)] = &[
            (1 | (1 << 3) | (1 << 10), 40),           // NETWORK + WITNESS + LIMITED
            (1 | (1 << 3) | (1 << 6), 20),            // NETWORK + WITNESS + COMPACT_FILTERS
            (1 | (1 << 3) | (1 << 24), 15),           // NETWORK + WITNESS + P2P_V2
            (1 | (1 << 3) | (1 << 27), 8),            // NETWORK + WITNESS + REDUCED_DATA
            (1 | (1 << 3) | (1 << 26), 10),           // NETWORK + WITNESS + FULL_RBF
            (1 | (1 << 3), 5),                         // NETWORK + WITNESS basic
            ((1 << 3) | (1 << 10), 2),                 // WITNESS + LIMITED only
        ];
        let mut idx = 0u64;
        for (ni, &net) in nets.iter().enumerate() {
            let count = counts[ni];
            for i in 0..count {
                let age_bucket = i % 7;
                let age = match age_bucket {
                    0 => 600 + (i % 3000),
                    1 => 5000 + (i % 10000),
                    2 => 50000 + (i % 30000),
                    3 => 200000 + (i % 400000),
                    4 => 1000000 + (i % 1500000),
                    5 => 4000000 + (i % 3000000),
                    _ => 8000000 + (i % 5000000),
                };
                let svc_pick = (idx as usize) % 100;
                let mut cumulative = 0u32;
                let mut services = svc_templates[0].0;
                for &(svc, pct) in svc_templates {
                    cumulative += pct;
                    if svc_pick < cumulative as usize { services = svc; break; }
                }
                addrs.push(KnownAddress {
                    time: now.saturating_sub(age),
                    services,
                    address: format!("{}-addr-{}", net, i),
                    port: 8333,
                    network: net.into(),
                });
                idx += 1;
            }
        }
        addrs
    }

    fn demo_recent_blocks(tip: u64, now: u64) -> Vec<BlockInfo> {
        (0..8).map(|i| {
            let height = tip - i;
            BlockInfo {
                height,
                hash: format!("{:064x}", height),
                size: 1_500_000 + (i * 50_000),
                weight: 3_800_000 + (i * 20_000),
                tx_count: 2800 - (i as usize * 100),
                time: now - (i * 600 + 120),
                version: 0x20000010, // BIP9 + bit 4 (BIP110)
            }
        }).collect()
    }

    /// Deterministic pseudo-random from seed (xorshift-style, cheap mixing).
    fn prng(seed: u64) -> u64 {
        let mut x = seed.wrapping_mul(2654435761).wrapping_add(1013904223);
        x ^= x >> 16; x = x.wrapping_mul(0x45d9f3b); x ^= x >> 16;
        x
    }

    fn demo_block_stats(height: u64, time: u64, day_offset: u64) -> BlockStats {
        // Use height as deterministic random seed for per-block jitter
        let r = |n: u64| (prng(height.wrapping_add(n)) % 1000) as f64 / 1000.0; // 0.0..1.0

        // --- Trends over 30 days (day 0 = most recent, day 29 = oldest) ---
        let d = day_offset as f64;

        // Runes: surging recently. 2% old -> 10% recent, with noise
        let rune_pct = 0.02 + 0.08 * (1.0 - d / 30.0).powi(2) + 0.03 * (r(1) - 0.5);

        // Inscriptions: declining from old peak. 8% old -> 3% recent
        let insc_pct = 0.03 + 0.05 * (d / 30.0) + 0.02 * (r(2) - 0.5);

        // BRC-20: brief spike around days 18-22, otherwise low
        let brc20_spike = (-(((d - 20.0) / 2.5).powi(2))).exp() * 0.04;
        let brc20_pct = 0.005 + brc20_spike + 0.005 * r(3);

        // OPNET: growing steadily. 0.5% old -> 6% recent with high variance
        let opnet_pct = 0.005 + 0.055 * (1.0 - d / 30.0) + 0.025 * (r(4) - 0.5);

        // Stamps: steady low ~0.5-1.5%
        let stamp_pct = 0.005 + 0.01 * r(5);

        // Counterparty: sporadic, mostly absent. occasional spikes
        let cp_pct = if r(6) > 0.7 { 0.002 + 0.008 * r(7) } else { 0.001 * r(7) };

        // Omni: nearly extinct, tiny blips
        let omni_pct = if r(8) > 0.85 { 0.001 + 0.003 * r(9) } else { 0.0002 * r(9) };

        // OP_RET other: steady background 1-3%
        let opret_pct = 0.01 + 0.02 * r(10);

        // Clamp all to non-negative
        let clamp = |v: f64| v.max(0.0);
        let rune_pct = clamp(rune_pct);
        let insc_pct = clamp(insc_pct);
        let brc20_pct = clamp(brc20_pct);
        let opnet_pct = clamp(opnet_pct);
        let stamp_pct = clamp(stamp_pct);
        let cp_pct = clamp(cp_pct);
        let omni_pct = clamp(omni_pct);
        let opret_pct = clamp(opret_pct);

        let data_pct = rune_pct + insc_pct + brc20_pct + opnet_pct + stamp_pct + cp_pct + omni_pct + opret_pct;
        let _fin_pct = (1.0 - data_pct).max(0.5); // financial is remainder

        let total_vsize = 950_000 + (prng(height) % 100_000);
        let tv = total_vsize as f64;
        let rune_vsize = (tv * rune_pct) as u64;
        let insc_vsize = (tv * insc_pct) as u64;
        let brc20_vsize = (tv * brc20_pct) as u64;
        let opnet_vsize = (tv * opnet_pct) as u64;
        let stamp_vsize = (tv * stamp_pct) as u64;
        let cp_vsize = (tv * cp_pct) as u64;
        let omni_vsize = (tv * omni_pct) as u64;
        let opret_vsize = (tv * opret_pct) as u64;
        let fin_vsize = total_vsize.saturating_sub(
            rune_vsize + insc_vsize + brc20_vsize + opnet_vsize +
            stamp_vsize + cp_vsize + omni_vsize + opret_vsize);

        // Counts roughly proportional to vsize (data txs are heavier on average)
        let tx_count = 2400 + (prng(height.wrapping_add(50)) % 800) as usize;
        let vsize_to_count = |vs: u64, avg_size: u64| (vs / avg_size.max(1)) as usize;
        let rune_count = vsize_to_count(rune_vsize, 800).max(1);
        let insc_count = vsize_to_count(insc_vsize, 2000).max(1);
        let brc20_count = vsize_to_count(brc20_vsize, 1500).max(1);
        let opnet_count = vsize_to_count(opnet_vsize, 1200).max(1);
        let stamp_count = vsize_to_count(stamp_vsize, 600).max(1);
        let cp_count = vsize_to_count(cp_vsize, 500).max(if cp_vsize > 0 { 1 } else { 0 });
        let omni_count = vsize_to_count(omni_vsize, 400).max(if omni_vsize > 0 { 1 } else { 0 });
        let opret_count = vsize_to_count(opret_vsize, 300).max(1);
        let data_count = rune_count + insc_count + brc20_count + opnet_count +
            stamp_count + cp_count + omni_count + opret_count;
        let financial_count = tx_count.saturating_sub(1).saturating_sub(data_count);

        // BIP-110 violators: inscriptions, OPNET, BRC-20 always violate; runes sometimes
        let rune_violators = (rune_count as f64 * 0.15 * (1.0 + r(20))) as usize;
        let violating = insc_count + opnet_count + brc20_count + rune_violators;
        let violating_vsize = insc_vsize + opnet_vsize + brc20_vsize +
            (rune_vsize as f64 * 0.15) as u64;
        let violating_size = violating_vsize * 3 / 2;

        // BIP-110 rule matrix
        let mut matrix = [[0usize; 7]; 10];
        matrix[3][0] = insc_count; matrix[3][1] = insc_count;   // inscriptions: R1, R2
        matrix[4][1] = opnet_count; matrix[4][6] = opnet_count; // opnet: R2, R7
        matrix[2][1] = brc20_count;                              // brc20: R2
        matrix[1][0] = rune_violators;                           // runes: R1 (oversized OP_RETURN)

        BlockStats {
            height,
            time,
            total_out: 200_000_000_000 + (prng(height.wrapping_add(30)) % 100_000_000_000),
            total_fee: 500_000 + (prng(height.wrapping_add(31)) % 1_500_000),
            tx_count,
            financial_count,
            rune_count,
            brc20_count,
            inscription_count: insc_count,
            opnet_count,
            stamp_count,
            counterparty_count: cp_count,
            omni_count,
            opreturn_other_count: opret_count,
            other_data_count: 0,
            total_vsize,
            financial_vsize: fin_vsize,
            rune_vsize,
            brc20_vsize,
            inscription_vsize: insc_vsize,
            opnet_vsize,
            stamp_vsize,
            counterparty_vsize: cp_vsize,
            omni_vsize,
            opreturn_other_vsize: opret_vsize,
            other_data_vsize: 0,
            oversized_opreturn_count: rune_count + opret_count,
            max_opreturn_size: 80 + (prng(height.wrapping_add(40)) % 60) as usize,
            max_spk_size: 34,
            max_witness_item_size: 400 + (prng(height.wrapping_add(41)) % 200) as usize,
            taproot_spend_count: 300 + (prng(height.wrapping_add(42)) % 400) as usize,
            taproot_output_count: 250 + (prng(height.wrapping_add(43)) % 350) as usize,
            bip110_checked: true,
            bip110_oversized_spk: 0,
            bip110_oversized_pushdata: insc_count + opnet_count + brc20_count,
            bip110_undefined_version: 0,
            bip110_annex: 0,
            bip110_oversized_control: 0,
            bip110_op_success: 0,
            bip110_op_if: opnet_count,
            bip110_violating_txs: violating,
            bip110_violating_vsize: violating_vsize,
            bip110_violating_size: violating_size,
            bip110_per_protocol: true,
            financial_bip110v: 0,
            rune_bip110v: rune_violators,
            brc20_bip110v: brc20_count,
            inscription_bip110v: insc_count,
            opnet_bip110v: opnet_count,
            stamp_bip110v: 0,
            counterparty_bip110v: 0,
            omni_bip110v: 0,
            opreturn_other_bip110v: 0,
            bip110_rule_matrix: matrix,
        }
    }

    fn demo_signaling(tip: u64, now: u64) -> NodeData {
        // Generate 2016 block versions
        let mut versions = Vec::new();
        for i in 0..2016u64 {
            let height = tip - i;
            // ~8% signal bit 4 (BIP110), bits 13-28 random (~50% each from ASICBoost)
            let mut version: i64 = 0x20000000;
            if height % 13 == 0 { version |= 1 << 4; } // ~8% BIP110
            // ASICBoost bits
            let pseudo = ((height * 2654435761) >> 16) as i64;
            for bit in 13..=28 {
                if pseudo & (1i64 << (bit - 13)) != 0 { version |= 1i64 << bit; }
            }
            versions.push((height, version));
        }

        let blockchain = BlockchainInfo {
            chain: "main".into(),
            blocks: tip,
            headers: tip,
            verificationprogress: 0.9999998,
            time: now - 320,
            softforks: demo_softforks(),
            ..Default::default()
        };

        let network = NetworkInfo {
            version: 270100,
            subversion: "/Knots:27.1.0/".into(),
            protocolversion: 70016,
            connections: 42,
            localservices: "0000000808000409".into(),
            ..Default::default()
        };

        NodeData {
            blockchain,
            network,
            uptime: 432_000,
            fetched_at: now,
            softforks: demo_softforks(),
            recent_block_versions: versions,
            ..Default::default()
        }
    }
}
