use crate::rpc::{BlockInfo, BlockStats, RpcClient};
use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::Arc;
use tokio::sync::{mpsc, Notify};

pub struct AppService {
    client: RpcClient,
    poll_notify: Arc<Notify>,
    signaling_notify: Arc<Notify>,
    force_full_fetch: Arc<AtomicBool>,
    backfill_stop: Arc<AtomicBool>,
    current_screen: Arc<AtomicU8>,
    stats_tx: mpsc::Sender<BlockStats>,
    older_blocks_tx: mpsc::Sender<Vec<BlockInfo>>,
}

impl AppService {
    pub fn new(
        client: RpcClient,
        poll_notify: Arc<Notify>,
        signaling_notify: Arc<Notify>,
        force_full_fetch: Arc<AtomicBool>,
        backfill_stop: Arc<AtomicBool>,
        current_screen: Arc<AtomicU8>,
        stats_tx: mpsc::Sender<BlockStats>,
        older_blocks_tx: mpsc::Sender<Vec<BlockInfo>>,
    ) -> Self {
        Self {
            client, poll_notify, signaling_notify, force_full_fetch,
            backfill_stop, current_screen, stats_tx, older_blocks_tx,
        }
    }

    pub fn force_refresh(&self) {
        self.force_full_fetch.store(true, Ordering::Relaxed);
        self.poll_notify.notify_one();
    }

    pub fn notify_poll(&self) {
        self.poll_notify.notify_one();
    }

    pub fn notify_signaling(&self) {
        self.signaling_notify.notify_one();
    }

    pub fn set_screen(&self, idx: u8) {
        self.current_screen.store(idx, Ordering::Relaxed);
    }

    pub fn fetch_older_blocks(&self, start: u64, end: u64) {
        let heights: Vec<u64> = (start..=end).rev().collect();
        let c = self.client.clone();
        let tx = self.older_blocks_tx.clone();
        tokio::spawn(async move {
            if let Ok(blocks) = c.fetch_block_infos(&heights).await {
                let _ = tx.send(blocks).await;
            }
        });
    }

    pub fn stop_backfill(&self) {
        self.backfill_stop.store(true, Ordering::Relaxed);
    }

    pub fn spawn_backfill(
        &self,
        recent_blocks: &[(u64, String)],
        analytics_heights: Vec<u64>,
        cached: &HashSet<u64>,
    ) -> u64 {
        let recent: Vec<(u64, String)> = recent_blocks
            .iter()
            .filter(|(h, _)| !cached.contains(h))
            .cloned()
            .collect();
        let recent_heights: HashSet<u64> = recent.iter().map(|(h, _)| *h).collect();
        let backfill: Vec<u64> = analytics_heights
            .into_iter()
            .filter(|h| !cached.contains(h) && !recent_heights.contains(h))
            .collect();

        let total = (recent.len() + backfill.len()) as u64;
        if total == 0 {
            return 0;
        }

        let c = self.client.clone();
        let tx = self.stats_tx.clone();
        let stop = self.backfill_stop.clone();
        tokio::spawn(async move {
            for (height, hash) in recent {
                if stop.load(Ordering::Relaxed) { break; }
                if let Ok(stats) = c.fetch_block_stats(&[(height, hash)]).await {
                    for s in stats {
                        let _ = tx.send(s).await;
                    }
                }
            }
            for height in backfill {
                if stop.load(Ordering::Relaxed) { break; }
                if let Ok(stat) = c.fetch_block_stats_by_height(height).await {
                    let _ = tx.send(stat).await;
                }
            }
        });

        total
    }

    pub fn fetch_new_block_stats(&self, blocks: Vec<(u64, String)>) {
        if blocks.is_empty() { return; }
        let c = self.client.clone();
        let tx = self.stats_tx.clone();
        tokio::spawn(async move {
            for (height, hash) in blocks {
                if let Ok(stats) = c.fetch_block_stats(&[(height, hash)]).await {
                    for s in stats {
                        let _ = tx.send(s).await;
                    }
                }
            }
        });
    }
}
