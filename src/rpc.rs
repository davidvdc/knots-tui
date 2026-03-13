use base64::Engine;
use reqwest::Client;
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::BTreeMap;

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
            ..Default::default()
        })
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

    pub async fn fetch_signaling(&self) -> Result<NodeData, String> {
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

        // Fetch last 144 blocks (~1 day) to scan version bits
        let mut recent_block_versions = Vec::new();
        let mut block_hash = blockchain.bestblockhash.clone();
        for _ in 0..144 {
            if block_hash.is_empty() {
                break;
            }
            let block_val = self.call("getblock", json!([block_hash, 1])).await?;
            let height = block_val["height"].as_u64().unwrap_or(0);
            let version = block_val["version"].as_i64().unwrap_or(0);
            let prev = block_val["previousblockhash"]
                .as_str()
                .unwrap_or("")
                .to_string();
            recent_block_versions.push((height, version));
            block_hash = prev;
        }

        let softforks = blockchain.softforks.clone();

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
