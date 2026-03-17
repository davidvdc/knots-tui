use base64::Engine;
use reqwest::Client;
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU16, Ordering};
use std::sync::Arc;

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

        // Fetch recent blocks (last 8)
        let mut recent_blocks = Vec::new();
        let mut block_hash = blockchain.bestblockhash.clone();
        for _ in 0..8 {
            if block_hash.is_empty() {
                break;
            }
            let block_val = self.call("getblock", json!([block_hash])).await?;
            let height = block_val["height"].as_u64().unwrap_or(0);
            let size = block_val["size"].as_u64().unwrap_or(0);
            let weight = block_val["weight"].as_u64().unwrap_or(0);
            let tx_count = block_val["nTx"].as_u64().unwrap_or(
                block_val["tx"].as_array().map(|a| a.len() as u64).unwrap_or(0),
            ) as usize;
            let time = block_val["time"].as_u64().unwrap_or(0);
            let version = block_val["version"].as_i64().unwrap_or(0);
            let prev = block_val["previousblockhash"]
                .as_str()
                .unwrap_or("")
                .to_string();

            recent_blocks.push(BlockInfo {
                height,
                hash: block_hash,
                size,
                weight,
                tx_count,
                time,
                version,
            });
            block_hash = prev;
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

            let txs = batch[1]["tx"].as_array();
            let mut tx_count = 0usize;
            // Mutually exclusive protocol buckets:
            let mut rune_count = 0usize;
            let mut brc20_count = 0usize;
            let mut inscription_count = 0usize; // ordinals excl. BRC-20
            let mut opnet_count = 0usize;
            let mut stamp_count = 0usize;
            let mut counterparty_count = 0usize;
            let mut omni_count = 0usize;
            let mut opreturn_other_count = 0usize;
            let mut other_data_count = 0usize;
            let mut financial_count = 0usize;
            // Per-protocol vsize:
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
            // Non-exclusive metrics:
            let mut oversized_opreturn_count = 0usize;
            let mut max_opreturn_size = 0usize;
            let mut taproot_spend_count = 0usize;
            let mut taproot_output_count = 0usize;

            if let Some(txs) = txs {
                tx_count = txs.len();
                for tx in txs {
                    let is_coinbase = tx["vin"]
                        .as_array()
                        .map(|v| v.iter().any(|i| !i["coinbase"].is_null()))
                        .unwrap_or(false);
                    let vsize = tx["vsize"].as_u64().unwrap_or(0);
                    total_vsize += vsize;
                    if is_coinbase {
                        continue;
                    }

                    // --- Scan outputs ---
                    let mut has_opreturn = false;
                    let mut has_rune = false;
                    let mut has_counterparty = false;
                    let mut has_omni = false;
                    let mut has_multisig = false;
                    let mut has_taproot_output = false;
                    if let Some(outs) = tx["vout"].as_array() {
                        for o in outs {
                            let script_type = o["scriptPubKey"]["type"].as_str().unwrap_or("");
                            if script_type == "nulldata" {
                                has_opreturn = true;
                                let script_hex = o["scriptPubKey"]["hex"]
                                    .as_str()
                                    .unwrap_or("");
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
                                // OPNET: exactly 5 witness items, control block (item[4]) = 65 bytes (130 hex),
                                // tapscript (item[3]) contains "026f70" (2-byte push of "op")
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

                    // --- Taproot (non-exclusive, counted independently) ---
                    if has_taproot_spend { taproot_spend_count += 1; }
                    if has_taproot_output { taproot_output_count += 1; }

                    // --- Classify into exactly one bucket (priority order) ---
                    if has_brc20 {
                        brc20_count += 1; brc20_vsize += vsize;
                    } else if has_inscription {
                        inscription_count += 1; inscription_vsize += vsize;
                    } else if has_opnet {
                        opnet_count += 1; opnet_vsize += vsize;
                    } else if has_rune {
                        rune_count += 1; rune_vsize += vsize;
                    } else if has_counterparty {
                        counterparty_count += 1; counterparty_vsize += vsize;
                    } else if has_omni {
                        omni_count += 1; omni_vsize += vsize;
                    } else if has_multisig && !has_opreturn {
                        stamp_count += 1; stamp_vsize += vsize;
                    } else if has_opreturn {
                        opreturn_other_count += 1; opreturn_other_vsize += vsize;
                    } else {
                        financial_count += 1; financial_vsize += vsize;
                        continue; // not a data tx
                    }
                }
            }

            let user_tx = tx_count.saturating_sub(1); // exclude coinbase
            let data_count = rune_count + brc20_count + inscription_count
                + opnet_count + stamp_count + counterparty_count + omni_count
                + opreturn_other_count;
            // Sanity: if there are more data txs than user txs, clamp
            let financial_count = user_tx.saturating_sub(data_count);

            all_stats.push(BlockStats {
                height: *height,
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
            });
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
