use base64::Engine;
use reqwest::Client;
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU16, Ordering};
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq)]
pub enum TxCategory {
    Coinbase,
    Financial,
    Brc20,
    Inscription,
    Opnet,
    Rune,
    Counterparty,
    Omni,
    Stamp,
    OpReturnOther,
}

#[derive(Debug, Clone)]
pub struct TxClassification {
    pub category: TxCategory,
    pub vsize: u64,
    pub has_taproot_spend: bool,
    pub has_taproot_output: bool,
    pub oversized_opreturn_count: usize,
    pub max_opreturn_size: usize,
}

/// Classify a single transaction from its JSON representation (getblock verbosity 2)
pub fn classify_tx(tx: &Value) -> TxClassification {
    let is_coinbase = tx["vin"]
        .as_array()
        .map(|v| v.iter().any(|i| !i["coinbase"].is_null()))
        .unwrap_or(false);
    let vsize = tx["vsize"].as_u64().unwrap_or(0);

    if is_coinbase {
        return TxClassification {
            category: TxCategory::Coinbase,
            vsize,
            has_taproot_spend: false,
            has_taproot_output: false,
            oversized_opreturn_count: 0,
            max_opreturn_size: 0,
        };
    }

    // --- Scan outputs ---
    let mut has_opreturn = false;
    let mut has_rune = false;
    let mut has_counterparty = false;
    let mut has_omni = false;
    let mut has_multisig = false;
    let mut has_taproot_output = false;
    let mut oversized_opreturn_count = 0usize;
    let mut max_opreturn_size = 0usize;
    if let Some(outs) = tx["vout"].as_array() {
        for o in outs {
            let script_type = o["scriptPubKey"]["type"].as_str().unwrap_or("");
            if script_type == "nulldata" {
                has_opreturn = true;
                let script_hex = o["scriptPubKey"]["hex"].as_str().unwrap_or("");
                let script_size = script_hex.len() / 2;
                if script_size > max_opreturn_size {
                    max_opreturn_size = script_size;
                }
                if script_size > 83 {
                    oversized_opreturn_count += 1;
                }
                if script_hex.starts_with("6a5d") {
                    has_rune = true;
                }
                if script_hex.contains("434e545250525459") {
                    has_counterparty = true;
                }
                if script_hex.contains("6f6d6e69") {
                    has_omni = true;
                }
            }
            if script_type == "multisig" {
                has_multisig = true;
            }
            if script_type == "witness_v1_taproot" {
                has_taproot_output = true;
            }
        }
    }

    // --- Scan inputs/witness ---
    let mut has_inscription = false;
    let mut has_brc20 = false;
    let mut has_opnet = false;
    let mut has_taproot_spend = false;
    if let Some(ins) = tx["vin"].as_array() {
        for input in ins {
            if input["prevout"]["scriptPubKey"]["type"].as_str()
                == Some("witness_v1_taproot")
            {
                has_taproot_spend = true;
            }
            if let Some(witness) = input["txinwitness"].as_array() {
                if witness.len() == 5 {
                    let ctrl = witness[4].as_str().unwrap_or("");
                    let tapscript = witness[3].as_str().unwrap_or("");
                    if ctrl.len() == 130 && tapscript.contains("026f70") {
                        has_opnet = true;
                    }
                }
                for item in witness {
                    if let Some(hex) = item.as_str() {
                        if hex.contains("0063036f7264") {
                            has_inscription = true;
                            if hex.contains("6272632d3230") {
                                has_brc20 = true;
                            }
                        } else if hex.len() > 1040 {
                            has_inscription = true;
                        }
                    }
                }
            }
        }
    }

    // --- Classify into exactly one bucket (priority order) ---
    let category = if has_brc20 {
        TxCategory::Brc20
    } else if has_inscription {
        TxCategory::Inscription
    } else if has_opnet {
        TxCategory::Opnet
    } else if has_rune {
        TxCategory::Rune
    } else if has_counterparty {
        TxCategory::Counterparty
    } else if has_omni {
        TxCategory::Omni
    } else if has_multisig && !has_opreturn {
        TxCategory::Stamp
    } else if has_opreturn {
        TxCategory::OpReturnOther
    } else {
        TxCategory::Financial
    };

    TxClassification {
        category,
        vsize,
        has_taproot_spend,
        has_taproot_output,
        oversized_opreturn_count,
        max_opreturn_size,
    }
}

/// Classify all transactions in a block and produce BlockStats
pub fn classify_block(txs: &[Value], total_out: u64, total_fee: u64, height: u64, block_time: u64) -> BlockStats {
    let tx_count = txs.len();
    let mut rune_count = 0usize;
    let mut brc20_count = 0usize;
    let mut inscription_count = 0usize;
    let mut opnet_count = 0usize;
    let mut stamp_count = 0usize;
    let mut counterparty_count = 0usize;
    let mut omni_count = 0usize;
    let mut opreturn_other_count = 0usize;
    let mut financial_count = 0usize;
    let mut total_vsize = 0u64;
    let mut financial_vsize = 0u64;
    let mut rune_vsize = 0u64;
    let mut brc20_vsize = 0u64;
    let mut inscription_vsize = 0u64;
    let mut opnet_vsize = 0u64;
    let mut stamp_vsize = 0u64;
    let mut counterparty_vsize = 0u64;
    let mut omni_vsize = 0u64;
    let mut opreturn_other_vsize = 0u64;
    let mut oversized_opreturn_count = 0usize;
    let mut max_opreturn_size = 0usize;
    let mut taproot_spend_count = 0usize;
    let mut taproot_output_count = 0usize;

    for tx in txs {
        let c = classify_tx(tx);
        total_vsize += c.vsize;
        if c.oversized_opreturn_count > 0 {
            oversized_opreturn_count += c.oversized_opreturn_count;
        }
        if c.max_opreturn_size > max_opreturn_size {
            max_opreturn_size = c.max_opreturn_size;
        }

        match c.category {
            TxCategory::Coinbase => continue,
            TxCategory::Brc20 => { brc20_count += 1; brc20_vsize += c.vsize; }
            TxCategory::Inscription => { inscription_count += 1; inscription_vsize += c.vsize; }
            TxCategory::Opnet => { opnet_count += 1; opnet_vsize += c.vsize; }
            TxCategory::Rune => { rune_count += 1; rune_vsize += c.vsize; }
            TxCategory::Counterparty => { counterparty_count += 1; counterparty_vsize += c.vsize; }
            TxCategory::Omni => { omni_count += 1; omni_vsize += c.vsize; }
            TxCategory::Stamp => { stamp_count += 1; stamp_vsize += c.vsize; }
            TxCategory::OpReturnOther => { opreturn_other_count += 1; opreturn_other_vsize += c.vsize; }
            TxCategory::Financial => { financial_count += 1; financial_vsize += c.vsize; }
        }

        if c.has_taproot_spend { taproot_spend_count += 1; }
        if c.has_taproot_output { taproot_output_count += 1; }
    }

    let user_tx = tx_count.saturating_sub(1);
    let data_count = rune_count + brc20_count + inscription_count
        + opnet_count + stamp_count + counterparty_count + omni_count
        + opreturn_other_count;
    let financial_count = user_tx.saturating_sub(data_count);

    BlockStats {
        height,
        time: block_time,
        total_out,
        total_fee,
        tx_count,
        financial_count,
        rune_count,
        brc20_count,
        inscription_count,
        opnet_count,
        stamp_count,
        counterparty_count,
        omni_count,
        opreturn_other_count,
        other_data_count: 0,
        total_vsize,
        financial_vsize,
        rune_vsize,
        brc20_vsize,
        inscription_vsize,
        opnet_vsize,
        stamp_vsize,
        counterparty_vsize,
        omni_vsize,
        opreturn_other_vsize,
        other_data_vsize: 0,
        oversized_opreturn_count,
        max_opreturn_size,
        taproot_spend_count,
        taproot_output_count,
    }
}

#[derive(Clone)]
pub struct RpcClient {
    url: String,
    auth_header: String,
    client: Client,
}

#[derive(Default, Clone, Debug)]
pub struct NodeData {
    pub error: Option<String>,
    pub blockchain: BlockchainInfo,
    pub network: NetworkInfo,
    pub mempool: MempoolInfo,
    pub mining: MiningInfo,
    pub peers: Vec<PeerInfo>,
    pub net_totals: NetTotals,
    pub uptime: u64,
    pub recent_blocks: Vec<BlockInfo>,
    pub fetched_at: u64,
    pub known_peers: u64,
    pub known_addresses: Vec<KnownAddress>,
    pub softforks: BTreeMap<String, SoftFork>,
    pub block_stats: Vec<BlockStats>,
    pub recent_block_versions: Vec<(u64, i64)>, // (height, version)
    pub ibd_blocks_per_sec: f64,  // sync speed (blocks/s), 0 until second fetch
    pub ibd_recv_per_sec: u64,    // download rate (bytes/s), 0 until second fetch
}

#[derive(Default, Clone, Debug, Deserialize)]
pub struct KnownAddress {
    #[serde(default)]
    pub time: u64,
    #[serde(default)]
    pub services: u64,
    #[serde(default)]
    pub address: String,
    #[serde(default)]
    pub port: u16,
    #[serde(default)]
    pub network: String,
}

#[derive(Default, Clone, Debug, Deserialize)]
pub struct BlockchainInfo {
    #[serde(default)]
    pub chain: String,
    #[serde(default)]
    pub blocks: u64,
    #[serde(default)]
    pub headers: u64,
    #[serde(default)]
    pub bestblockhash: String,
    #[serde(default)]
    pub difficulty: f64,
    #[serde(default)]
    pub time: u64,
    #[serde(default)]
    pub mediantime: u64,
    #[serde(default)]
    pub verificationprogress: f64,
    #[serde(default)]
    pub initialblockdownload: bool,
    #[serde(default)]
    pub size_on_disk: u64,
    #[serde(default)]
    pub pruned: bool,
    #[serde(default)]
    pub warnings: WarningsField,
    #[serde(default)]
    pub softforks: BTreeMap<String, SoftFork>,
}

#[derive(Default, Clone, Debug, Deserialize)]
pub struct SoftFork {
    #[serde(default, rename = "type")]
    pub fork_type: String,
    #[serde(default)]
    pub bip9: Option<Bip9Info>,
    #[serde(default)]
    pub height: Option<i64>,
    #[serde(default)]
    pub active: bool,
}

#[derive(Default, Clone, Debug, Deserialize)]
pub struct Bip9Info {
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub bit: Option<u8>,
    #[serde(default)]
    pub start_time: i64,
    #[serde(default)]
    pub timeout: i64,
    #[serde(default)]
    pub since: u64,
    #[serde(default)]
    pub statistics: Option<Bip9Statistics>,
}

#[derive(Default, Clone, Debug, Deserialize)]
pub struct Bip9Statistics {
    #[serde(default)]
    pub period: u64,
    #[serde(default)]
    pub threshold: u64,
    #[serde(default)]
    pub elapsed: u64,
    #[serde(default)]
    pub count: u64,
    #[serde(default)]
    pub possible: bool,
}

#[derive(Default, Clone, Debug, Deserialize)]
#[serde(untagged)]
pub enum WarningsField {
    #[default]
    None,
    Single(String),
    Multiple(Vec<String>),
}

impl WarningsField {
    pub fn as_str(&self) -> String {
        match self {
            WarningsField::None => String::new(),
            WarningsField::Single(s) => s.clone(),
            WarningsField::Multiple(v) => v.join("; "),
        }
    }
}

#[derive(Default, Clone, Debug, Deserialize)]
pub struct NetworkInfo {
    #[serde(default)]
    pub version: u64,
    #[serde(default)]
    pub subversion: String,
    #[serde(default)]
    pub protocolversion: u64,
    #[serde(default)]
    pub connections: u64,
    #[serde(default)]
    pub connections_in: u64,
    #[serde(default)]
    pub connections_out: u64,
    #[serde(default)]
    pub relayfee: f64,
    #[serde(default)]
    pub incrementalfee: f64,
    #[serde(default)]
    pub localservices: String,
    #[serde(default)]
    pub localaddresses: Vec<LocalAddress>,
    #[serde(default)]
    pub warnings: WarningsField,
}

#[derive(Default, Clone, Debug, Deserialize)]
pub struct LocalAddress {
    #[serde(default)]
    pub address: String,
    #[serde(default)]
    pub port: u16,
}

#[derive(Default, Clone, Debug, Deserialize)]
pub struct MempoolInfo {
    #[serde(default)]
    pub loaded: bool,
    #[serde(default)]
    pub size: u64,
    #[serde(default)]
    pub bytes: u64,
    #[serde(default)]
    pub usage: u64,
    #[serde(default)]
    pub total_fee: f64,
    #[serde(default)]
    pub maxmempool: u64,
    #[serde(default)]
    pub mempoolminfee: f64,
    #[serde(default)]
    pub minrelaytxfee: f64,
}

#[derive(Default, Clone, Debug, Deserialize)]
pub struct MiningInfo {
    #[serde(default)]
    pub networkhashps: f64,
    #[serde(default)]
    pub pooledtx: u64,
    #[serde(default)]
    pub chain: String,
}

#[derive(Default, Clone, Debug, Deserialize)]
pub struct PeerInfo {
    #[serde(default)]
    pub id: u64,
    #[serde(default)]
    pub addr: String,
    #[serde(default)]
    pub subver: String,
    #[serde(default)]
    pub version: u64,
    #[serde(default)]
    pub inbound: bool,
    #[serde(default)]
    pub startingheight: i64,
    #[serde(default)]
    pub synced_headers: i64,
    #[serde(default)]
    pub synced_blocks: i64,
    #[serde(default)]
    pub pingtime: Option<f64>,
    #[serde(default)]
    pub bytessent: u64,
    #[serde(default)]
    pub bytesrecv: u64,
    #[serde(default)]
    pub connection_type: String,
    #[serde(default)]
    pub conntime: u64,
    #[serde(default)]
    pub lastsend: u64,
    #[serde(default)]
    pub lastrecv: u64,
    #[serde(default)]
    pub relaytxes: bool,
}

#[derive(Default, Clone, Debug, Deserialize)]
pub struct NetTotals {
    #[serde(default)]
    pub totalbytesrecv: u64,
    #[serde(default)]
    pub totalbytessent: u64,
}

#[derive(Default, Clone, Debug)]
pub struct BlockInfo {
    pub height: u64,
    pub hash: String,
    pub size: u64,
    pub weight: u64,
    pub tx_count: usize,
    pub time: u64,
    pub version: i64,
}

#[derive(Default, Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct BlockStats {
    pub height: u64,
    pub time: u64,                // block timestamp
    pub total_out: u64,           // total BTC output in satoshis
    pub total_fee: u64,           // total fees in satoshis
    pub tx_count: usize,          // total transactions (incl coinbase)
    pub financial_count: usize,   // financial transactions (no data patterns)
    // Mutually exclusive protocol categories (each tx counted once):
    pub rune_count: usize,        // OP_RETURN with OP_13 tag
    pub brc20_count: usize,       // inscription with "brc-20" payload
    pub inscription_count: usize, // ordinals envelope (excl. BRC-20)
    pub opnet_count: usize,       // OPNET/BSI (tapscript with "op" magic)
    pub stamp_count: usize,       // bare multisig (no OP_RETURN/inscription)
    pub counterparty_count: usize, // OP_RETURN with CNTRPRTY prefix
    pub omni_count: usize,        // OP_RETURN with omni prefix
    pub opreturn_other_count: usize, // OP_RETURN not matching known protocols
    pub other_data_count: usize,  // unclassified data tx
    // Per-protocol vsize (virtual bytes):
    #[serde(default)] pub total_vsize: u64,
    #[serde(default)] pub financial_vsize: u64,
    #[serde(default)] pub rune_vsize: u64,
    #[serde(default)] pub brc20_vsize: u64,
    #[serde(default)] pub inscription_vsize: u64,
    #[serde(default)] pub opnet_vsize: u64,
    #[serde(default)] pub stamp_vsize: u64,
    #[serde(default)] pub counterparty_vsize: u64,
    #[serde(default)] pub omni_vsize: u64,
    #[serde(default)] pub opreturn_other_vsize: u64,
    #[serde(default)] pub other_data_vsize: u64,
    // Non-exclusive metrics:
    pub oversized_opreturn_count: usize, // OP_RETURNs exceeding 83-byte limit
    pub max_opreturn_size: usize,  // largest OP_RETURN scriptPubKey in bytes
    pub taproot_spend_count: usize,  // txs spending from taproot inputs
    pub taproot_output_count: usize, // txs creating taproot outputs
}

#[derive(Deserialize)]
struct RpcResponse {
    result: Option<Value>,
    error: Option<Value>,
}

impl RpcClient {
    pub fn new(url: &str, cookie: &str) -> Self {
        let auth = base64::engine::general_purpose::STANDARD.encode(cookie.as_bytes());
        Self {
            url: url.to_string(),
            auth_header: format!("Basic {}", auth),
            client: Client::new(),
        }
    }

    async fn call(&self, method: &str, params: Value) -> Result<Value, String> {
        let body = json!({
            "jsonrpc": "1.0",
            "id": method,
            "method": method,
            "params": params,
        });

        let resp = self
            .client
            .post(&self.url)
            .header("Authorization", &self.auth_header)
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("HTTP error: {}", e))?;

        let status = resp.status();
        let text = resp.text().await.map_err(|e| format!("Read error: {}", e))?;

        if !status.is_success() {
            return Err(format!("RPC HTTP {}: {}", status, text));
        }

        let rpc_resp: RpcResponse =
            serde_json::from_str(&text).map_err(|e| format!("JSON parse error: {}", e))?;

        if let Some(err) = rpc_resp.error {
            return Err(format!("RPC error: {}", err));
        }

        rpc_resp.result.ok_or_else(|| "null result".to_string())
    }

    /// Batch multiple RPC calls in one HTTP request
    async fn batch_call(&self, calls: &[(&str, Value)]) -> Result<Vec<Value>, String> {
        let body: Vec<Value> = calls
            .iter()
            .enumerate()
            .map(|(i, (method, params))| {
                json!({
                    "jsonrpc": "1.0",
                    "id": i,
                    "method": method,
                    "params": params,
                })
            })
            .collect();

        let resp = self
            .client
            .post(&self.url)
            .header("Authorization", &self.auth_header)
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("HTTP error: {}", e))?;

        let status = resp.status();
        let text = resp.text().await.map_err(|e| format!("Read error: {}", e))?;

        if !status.is_success() {
            return Err(format!("RPC HTTP {}: {}", status, text));
        }

        let responses: Vec<RpcResponse> =
            serde_json::from_str(&text).map_err(|e| format!("JSON parse error: {}", e))?;

        let mut results = Vec::new();
        for r in responses {
            if let Some(err) = r.error {
                return Err(format!("RPC error: {}", err));
            }
            results.push(r.result.unwrap_or(Value::Null));
        }
        Ok(results)
    }

    /// Cheap check: returns (block_height, connection_count) with minimal RPC overhead
    pub async fn fetch_tip_and_peers(&self) -> Result<(u64, u64), String> {
        let results = self
            .batch_call(&[
                ("getblockcount", json!([])),
                ("getconnectioncount", json!([])),
            ])
            .await?;
        let height = results[0].as_u64().unwrap_or(0);
        let conns = results[1].as_u64().unwrap_or(0);
        Ok((height, conns))
    }

    pub async fn fetch_dashboard(&self) -> Result<NodeData, String> {
        let now = chrono::Utc::now().timestamp() as u64;

        let batch_results = self
            .batch_call(&[
                ("getblockchaininfo", json!([])),
                ("getnetworkinfo", json!([])),
                ("getmempoolinfo", json!([])),
                ("getmininginfo", json!([])),
                ("getpeerinfo", json!([])),
                ("getnettotals", json!([])),
                ("uptime", json!([])),
                ("getnodeaddresses", json!([0])),
            ])
            .await?;

        let blockchain: BlockchainInfo =
            serde_json::from_value(batch_results[0].clone()).map_err(|e| e.to_string())?;
        let network: NetworkInfo =
            serde_json::from_value(batch_results[1].clone()).map_err(|e| e.to_string())?;
        let mempool: MempoolInfo =
            serde_json::from_value(batch_results[2].clone()).map_err(|e| e.to_string())?;
        let mining: MiningInfo =
            serde_json::from_value(batch_results[3].clone()).map_err(|e| e.to_string())?;
        let peers: Vec<PeerInfo> =
            serde_json::from_value(batch_results[4].clone()).map_err(|e| e.to_string())?;
        let net_totals: NetTotals =
            serde_json::from_value(batch_results[5].clone()).map_err(|e| e.to_string())?;
        let uptime: u64 =
            serde_json::from_value(batch_results[6].clone()).map_err(|e| e.to_string())?;
        let known_peers = batch_results[7]
            .as_array()
            .map(|a| a.len() as u64)
            .unwrap_or(0);

        // Fetch recent blocks (last 50) using batched RPC calls
        let mut recent_blocks = Vec::new();
        let tip = blockchain.blocks;
        let num_blocks = 50u64.min(tip + 1);
        let heights: Vec<u64> = (0..num_blocks).map(|i| tip - i).collect();

        // Batch getblockhash for all heights
        let hash_calls: Vec<(&str, Value)> = heights
            .iter()
            .map(|&h| ("getblockhash", json!([h])))
            .collect();
        let hash_results = self.batch_call(&hash_calls).await?;
        let hashes: Vec<String> = hash_results
            .iter()
            .map(|v| v.as_str().unwrap_or("").to_string())
            .collect();

        // Batch getblock for all hashes (verbosity 1 = JSON without raw tx)
        let block_calls: Vec<(&str, Value)> = hashes
            .iter()
            .map(|h| ("getblock", json!([h, 1])))
            .collect();
        let block_results = self.batch_call(&block_calls).await?;

        for (i, block_val) in block_results.iter().enumerate() {
            let height = block_val["height"].as_u64().unwrap_or(heights[i]);
            let size = block_val["size"].as_u64().unwrap_or(0);
            let weight = block_val["weight"].as_u64().unwrap_or(0);
            let tx_count = block_val["nTx"].as_u64().unwrap_or(
                block_val["tx"].as_array().map(|a| a.len() as u64).unwrap_or(0),
            ) as usize;
            let time = block_val["time"].as_u64().unwrap_or(0);
            let version = block_val["version"].as_i64().unwrap_or(0);

            recent_blocks.push(BlockInfo {
                height,
                hash: hashes[i].clone(),
                size,
                weight,
                tx_count,
                time,
                version,
            });
        }

        Ok(NodeData {
            error: None,
            blockchain,
            network,
            mempool,
            mining,
            peers,
            net_totals,
            uptime,
            recent_blocks,
            fetched_at: now,
            known_peers,
            ..Default::default()
        })
    }

    pub async fn fetch_block_stats(&self, hashes: &[(u64, String)]) -> Result<Vec<BlockStats>, String> {
        let mut all_stats = Vec::new();

        for (height, hash) in hashes {
            // Fetch getblockstats (lightweight) and getblock verbosity 2 (full tx data)
            let stats_fields = json!(["total_out", "totalfee"]);
            let batch = self.batch_call(&[
                ("getblockstats", json!([hash, stats_fields])),
                ("getblock", json!([hash, 2])),
            ]).await?;

            let total_out = batch[0]["total_out"].as_u64().unwrap_or(0);
            let total_fee = batch[0]["totalfee"].as_u64().unwrap_or(0);
            let block_time = batch[1]["time"].as_u64().unwrap_or(0);

            let txs = batch[1]["tx"].as_array()
                .map(|v| v.as_slice())
                .unwrap_or(&[]);
            all_stats.push(classify_block(txs, total_out, total_fee, *height, block_time));
        }

        Ok(all_stats)
    }

    /// Fetch block stats for a single block by height (for analytics backfill)
    pub async fn fetch_block_stats_by_height(&self, height: u64) -> Result<BlockStats, String> {
        let hash_val = self.call("getblockhash", json!([height])).await?;
        let hash = hash_val.as_str().unwrap_or("").to_string();
        let results = self.fetch_block_stats(&[(height, hash)]).await?;
        results.into_iter().next().ok_or_else(|| "no stats returned".to_string())
    }

    pub async fn fetch_known_peers(&self) -> Result<NodeData, String> {
        let now = chrono::Utc::now().timestamp() as u64;

        let batch_results = self
            .batch_call(&[
                ("getblockchaininfo", json!([])),
                ("getnetworkinfo", json!([])),
                ("uptime", json!([])),
                ("getnodeaddresses", json!([0])),
            ])
            .await?;

        let blockchain: BlockchainInfo =
            serde_json::from_value(batch_results[0].clone()).map_err(|e| e.to_string())?;
        let network: NetworkInfo =
            serde_json::from_value(batch_results[1].clone()).map_err(|e| e.to_string())?;
        let uptime: u64 =
            serde_json::from_value(batch_results[2].clone()).map_err(|e| e.to_string())?;
        let known_addresses: Vec<KnownAddress> =
            serde_json::from_value(batch_results[3].clone()).unwrap_or_default();
        let known_peers = known_addresses.len() as u64;

        Ok(NodeData {
            error: None,
            blockchain,
            network,
            uptime,
            fetched_at: now,
            known_peers,
            known_addresses,
            ..Default::default()
        })
    }

    pub async fn fetch_signaling(
        &self,
        progress: &Arc<AtomicU16>,
    ) -> Result<NodeData, String> {
        let now = chrono::Utc::now().timestamp() as u64;

        let batch_results = self
            .batch_call(&[
                ("getblockchaininfo", json!([])),
                ("getnetworkinfo", json!([])),
                ("uptime", json!([])),
            ])
            .await?;

        let blockchain: BlockchainInfo =
            serde_json::from_value(batch_results[0].clone()).map_err(|e| e.to_string())?;
        let network: NetworkInfo =
            serde_json::from_value(batch_results[1].clone()).map_err(|e| e.to_string())?;
        let uptime: u64 =
            serde_json::from_value(batch_results[2].clone()).map_err(|e| e.to_string())?;

        let softforks = blockchain.softforks.clone();

        // Fetch last 2016 block headers using batched RPC calls
        let tip_height = blockchain.blocks;
        let batch_size = 100usize;
        let total_blocks = 2016u64.min(tip_height);
        let mut recent_block_versions = Vec::new();

        // Process in chunks: batch getblockhash, then batch getblockheader
        let mut remaining = total_blocks as usize;
        let mut current_height = tip_height;

        while remaining > 0 {
            let chunk = remaining.min(batch_size);
            let heights: Vec<u64> = (0..chunk)
                .map(|i| current_height - i as u64)
                .collect();

            // Batch getblockhash for all heights in this chunk
            let hash_calls: Vec<(&str, Value)> = heights
                .iter()
                .map(|&h| ("getblockhash", json!([h])))
                .collect();
            let hash_results = self.batch_call(&hash_calls).await?;

            let hashes: Vec<String> = hash_results
                .iter()
                .map(|v| v.as_str().unwrap_or("").to_string())
                .collect();

            // Batch getblockheader for all hashes
            let header_calls: Vec<(&str, Value)> = hashes
                .iter()
                .map(|h| ("getblockheader", json!([h, true])))
                .collect();
            let header_results = self.batch_call(&header_calls).await?;

            for (i, header_val) in header_results.iter().enumerate() {
                let height = heights[i];
                let version = header_val["version"].as_i64().unwrap_or(0);
                recent_block_versions.push((height, version));
            }

            progress.store(recent_block_versions.len() as u16, Ordering::Relaxed);

            current_height = current_height.saturating_sub(chunk as u64);
            remaining -= chunk;
        }

        Ok(NodeData {
            error: None,
            blockchain,
            network,
            uptime,
            fetched_at: now,
            softforks,
            recent_block_versions,
            ..Default::default()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // --- WarningsField tests ---

    #[test]
    fn warnings_none() {
        let w: WarningsField = serde_json::from_value(json!(null)).unwrap_or_default();
        assert_eq!(w.as_str(), "");
    }

    #[test]
    fn warnings_single() {
        let w: WarningsField = serde_json::from_value(json!("some warning")).unwrap();
        assert_eq!(w.as_str(), "some warning");
    }

    #[test]
    fn warnings_multiple() {
        let w: WarningsField = serde_json::from_value(json!(["warn1", "warn2"])).unwrap();
        assert_eq!(w.as_str(), "warn1; warn2");
    }

    // --- BlockStats serde tests ---

    #[test]
    fn blockstats_roundtrip() {
        let stats = BlockStats {
            height: 800000,
            time: 1700000000,
            total_out: 5000000000,
            total_fee: 1000000,
            tx_count: 3000,
            financial_count: 2900,
            rune_count: 50,
            brc20_count: 10,
            inscription_count: 20,
            opnet_count: 5,
            stamp_count: 3,
            counterparty_count: 2,
            omni_count: 1,
            opreturn_other_count: 9,
            other_data_count: 0,
            total_vsize: 999000,
            financial_vsize: 900000,
            rune_vsize: 50000,
            brc20_vsize: 10000,
            inscription_vsize: 20000,
            opnet_vsize: 5000,
            stamp_vsize: 3000,
            counterparty_vsize: 2000,
            omni_vsize: 1000,
            opreturn_other_vsize: 8000,
            other_data_vsize: 0,
            oversized_opreturn_count: 5,
            max_opreturn_size: 200,
            taproot_spend_count: 100,
            taproot_output_count: 150,
        };
        let json_str = serde_json::to_string(&stats).unwrap();
        let deserialized: BlockStats = serde_json::from_str(&json_str).unwrap();
        assert_eq!(deserialized.height, stats.height);
        assert_eq!(deserialized.total_vsize, stats.total_vsize);
        assert_eq!(deserialized.rune_vsize, stats.rune_vsize);
    }

    #[test]
    fn blockstats_legacy_compat() {
        // JSON missing vsize fields should default to 0
        let json_str = r#"{"height":800000,"time":1700000000,"total_out":5000000000,"total_fee":1000000,"tx_count":3000,"financial_count":2900,"rune_count":50,"brc20_count":10,"inscription_count":20,"opnet_count":5,"stamp_count":3,"counterparty_count":2,"omni_count":1,"opreturn_other_count":9,"other_data_count":0,"oversized_opreturn_count":5,"max_opreturn_size":200,"taproot_spend_count":100,"taproot_output_count":150}"#;
        let stats: BlockStats = serde_json::from_str(json_str).unwrap();
        assert_eq!(stats.total_vsize, 0);
        assert_eq!(stats.financial_vsize, 0);
        assert_eq!(stats.rune_vsize, 0);
    }

    // --- classify_tx tests ---

    fn make_financial_tx() -> Value {
        json!({
            "vsize": 250,
            "vin": [{"txid": "abc", "vout": 0}],
            "vout": [{"scriptPubKey": {"type": "pubkeyhash", "hex": "76a914..."}}]
        })
    }

    fn make_coinbase_tx() -> Value {
        json!({
            "vsize": 200,
            "vin": [{"coinbase": "0123456789"}],
            "vout": [{"scriptPubKey": {"type": "pubkeyhash", "hex": "76a914..."}}]
        })
    }

    #[test]
    fn classify_coinbase() {
        let c = classify_tx(&make_coinbase_tx());
        assert_eq!(c.category, TxCategory::Coinbase);
        assert_eq!(c.vsize, 200);
    }

    #[test]
    fn classify_financial() {
        let c = classify_tx(&make_financial_tx());
        assert_eq!(c.category, TxCategory::Financial);
        assert_eq!(c.vsize, 250);
        assert!(!c.has_taproot_spend);
        assert!(!c.has_taproot_output);
    }

    #[test]
    fn classify_rune() {
        let tx = json!({
            "vsize": 300,
            "vin": [{"txid": "abc", "vout": 0}],
            "vout": [{"scriptPubKey": {"type": "nulldata", "hex": "6a5d0401020304"}}]
        });
        let c = classify_tx(&tx);
        assert_eq!(c.category, TxCategory::Rune);
    }

    #[test]
    fn classify_inscription() {
        let tx = json!({
            "vsize": 500,
            "vin": [{
                "txid": "abc", "vout": 0,
                "txinwitness": ["deadbeef", "0063036f726401010a746578742f706c61696e"]
            }],
            "vout": [{"scriptPubKey": {"type": "witness_v1_taproot", "hex": "5120..."}}]
        });
        let c = classify_tx(&tx);
        assert_eq!(c.category, TxCategory::Inscription);
    }

    #[test]
    fn classify_brc20() {
        // BRC-20 has ordinals envelope + "brc-20" payload
        let tx = json!({
            "vsize": 400,
            "vin": [{
                "txid": "abc", "vout": 0,
                "txinwitness": ["deadbeef", "0063036f72646272632d3230"]
            }],
            "vout": [{"scriptPubKey": {"type": "witness_v1_taproot", "hex": "5120..."}}]
        });
        let c = classify_tx(&tx);
        assert_eq!(c.category, TxCategory::Brc20);
    }

    #[test]
    fn classify_large_witness_inscription() {
        // Witness item > 1040 hex chars (520 bytes) → Inscription
        let large_hex = "ab".repeat(521); // 1042 hex chars
        let tx = json!({
            "vsize": 600,
            "vin": [{"txid": "abc", "vout": 0, "txinwitness": [large_hex]}],
            "vout": [{"scriptPubKey": {"type": "pubkeyhash", "hex": "76a914..."}}]
        });
        let c = classify_tx(&tx);
        assert_eq!(c.category, TxCategory::Inscription);
    }

    #[test]
    fn classify_opnet() {
        // OPNET: 5 witness items, control block 130 hex, tapscript contains "026f70"
        let ctrl = "a".repeat(130);
        let tx = json!({
            "vsize": 350,
            "vin": [{"txid": "abc", "vout": 0, "txinwitness": ["sig", "item1", "item2", "deadbeef026f70cafe", ctrl]}],
            "vout": [{"scriptPubKey": {"type": "pubkeyhash", "hex": "76a914..."}}]
        });
        let c = classify_tx(&tx);
        assert_eq!(c.category, TxCategory::Opnet);
    }

    #[test]
    fn classify_counterparty() {
        let tx = json!({
            "vsize": 250,
            "vin": [{"txid": "abc", "vout": 0}],
            "vout": [{"scriptPubKey": {"type": "nulldata", "hex": "6a28434e545250525459abcdef"}}]
        });
        let c = classify_tx(&tx);
        assert_eq!(c.category, TxCategory::Counterparty);
    }

    #[test]
    fn classify_omni() {
        let tx = json!({
            "vsize": 250,
            "vin": [{"txid": "abc", "vout": 0}],
            "vout": [{"scriptPubKey": {"type": "nulldata", "hex": "6a146f6d6e69abcdef"}}]
        });
        let c = classify_tx(&tx);
        assert_eq!(c.category, TxCategory::Omni);
    }

    #[test]
    fn classify_stamp() {
        // Bare multisig with no OP_RETURN
        let tx = json!({
            "vsize": 350,
            "vin": [{"txid": "abc", "vout": 0}],
            "vout": [{"scriptPubKey": {"type": "multisig", "hex": "5121..."}}]
        });
        let c = classify_tx(&tx);
        assert_eq!(c.category, TxCategory::Stamp);
    }

    #[test]
    fn classify_opreturn_other() {
        // OP_RETURN that doesn't match any known protocol
        let tx = json!({
            "vsize": 220,
            "vin": [{"txid": "abc", "vout": 0}],
            "vout": [{"scriptPubKey": {"type": "nulldata", "hex": "6a0c48656c6c6f20576f726c64"}}]
        });
        let c = classify_tx(&tx);
        assert_eq!(c.category, TxCategory::OpReturnOther);
    }

    #[test]
    fn classify_oversized_opreturn() {
        // OP_RETURN with >83 bytes (>166 hex chars)
        let hex = "6a".to_string() + &"ff".repeat(90); // 91 bytes total
        let tx = json!({
            "vsize": 250,
            "vin": [{"txid": "abc", "vout": 0}],
            "vout": [{"scriptPubKey": {"type": "nulldata", "hex": hex}}]
        });
        let c = classify_tx(&tx);
        assert_eq!(c.oversized_opreturn_count, 1);
        assert_eq!(c.max_opreturn_size, 91);
    }

    #[test]
    fn classify_taproot_tracking() {
        let tx = json!({
            "vsize": 150,
            "vin": [{"txid": "abc", "vout": 0, "prevout": {"scriptPubKey": {"type": "witness_v1_taproot"}}}],
            "vout": [{"scriptPubKey": {"type": "witness_v1_taproot", "hex": "5120..."}}]
        });
        let c = classify_tx(&tx);
        assert_eq!(c.category, TxCategory::Financial);
        assert!(c.has_taproot_spend);
        assert!(c.has_taproot_output);
    }

    // --- Priority tests ---

    #[test]
    fn priority_brc20_beats_inscription() {
        // A tx with both ordinals envelope and "brc-20" should be BRC-20, not Inscription
        let tx = json!({
            "vsize": 400,
            "vin": [{"txid": "abc", "vout": 0, "txinwitness": ["0063036f72646272632d3230"]}],
            "vout": [{"scriptPubKey": {"type": "pubkeyhash", "hex": "76a914..."}}]
        });
        let c = classify_tx(&tx);
        assert_eq!(c.category, TxCategory::Brc20);
    }

    #[test]
    fn priority_rune_beats_stamp() {
        // Tx with both rune OP_RETURN and multisig should be Rune
        let tx = json!({
            "vsize": 300,
            "vin": [{"txid": "abc", "vout": 0}],
            "vout": [
                {"scriptPubKey": {"type": "nulldata", "hex": "6a5d0401020304"}},
                {"scriptPubKey": {"type": "multisig", "hex": "5121..."}}
            ]
        });
        let c = classify_tx(&tx);
        assert_eq!(c.category, TxCategory::Rune);
    }

    #[test]
    fn multisig_with_opreturn_is_opreturn_other() {
        // Multisig + OP_RETURN (non-rune) → OpReturnOther, not Stamp
        let tx = json!({
            "vsize": 300,
            "vin": [{"txid": "abc", "vout": 0}],
            "vout": [
                {"scriptPubKey": {"type": "nulldata", "hex": "6a0c48656c6c6f"}},
                {"scriptPubKey": {"type": "multisig", "hex": "5121..."}}
            ]
        });
        let c = classify_tx(&tx);
        assert_eq!(c.category, TxCategory::OpReturnOther);
    }

    // --- classify_block tests ---

    #[test]
    fn classify_block_sums() {
        let coinbase = make_coinbase_tx();
        let fin1 = make_financial_tx();
        let fin2 = make_financial_tx();
        let rune = json!({
            "vsize": 300,
            "vin": [{"txid": "abc", "vout": 0}],
            "vout": [{"scriptPubKey": {"type": "nulldata", "hex": "6a5d0401020304"}}]
        });
        let txs = vec![coinbase, fin1, fin2, rune];
        let stats = classify_block(&txs, 5000000000, 100000, 800000, 1700000000);

        assert_eq!(stats.tx_count, 4);
        // user_tx = 3, data = 1 (rune), financial = 2
        assert_eq!(stats.financial_count, 2);
        assert_eq!(stats.rune_count, 1);
        // All other categories should be 0
        assert_eq!(stats.brc20_count, 0);
        assert_eq!(stats.inscription_count, 0);
        assert_eq!(stats.opnet_count, 0);
        assert_eq!(stats.stamp_count, 0);
        assert_eq!(stats.counterparty_count, 0);
        assert_eq!(stats.omni_count, 0);
        assert_eq!(stats.opreturn_other_count, 0);
        // Buckets sum to user_tx count
        let bucket_sum = stats.financial_count + stats.rune_count + stats.brc20_count
            + stats.inscription_count + stats.opnet_count + stats.stamp_count
            + stats.counterparty_count + stats.omni_count + stats.opreturn_other_count;
        assert_eq!(bucket_sum, stats.tx_count - 1); // minus coinbase
    }

    #[test]
    fn classify_block_empty() {
        let stats = classify_block(&[], 0, 0, 800000, 1700000000);
        assert_eq!(stats.tx_count, 0);
        assert_eq!(stats.financial_count, 0);
    }
}
