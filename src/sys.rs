use std::collections::HashMap;
use std::time::Instant;

#[derive(Clone, Default)]
pub struct CpuUsage {
    pub user_pct: f32,
    pub system_pct: f32,
    pub nice_pct: f32,
    pub iowait_pct: f32,
}

#[derive(Clone, Default)]
pub struct MemUsage {
    pub total: u64,
    pub used: u64,
    pub buffers: u64,
    pub cached: u64,
    pub swap_total: u64,
    pub swap_used: u64,
}

#[derive(Clone)]
pub struct DiskIO {
    pub name: String,
    pub read_per_sec: u64,
    pub write_per_sec: u64,
}

#[derive(Clone, Default)]
pub struct SystemStats {
    pub cpus: Vec<CpuUsage>,
    pub mem: MemUsage,
    pub disks: Vec<DiskIO>,
}

struct CpuSample {
    user: u64,
    nice: u64,
    system: u64,
    idle: u64,
    iowait: u64,
    irq: u64,
    softirq: u64,
    steal: u64,
}

impl CpuSample {
    fn total(&self) -> u64 {
        self.user + self.nice + self.system + self.idle
            + self.iowait + self.irq + self.softirq + self.steal
    }
}

struct DiskSample {
    sectors_read: u64,
    sectors_written: u64,
}

pub struct SystemSampler {
    prev_cpus: Vec<CpuSample>,
    prev_disks: HashMap<String, DiskSample>,
    prev_time: Instant,
}

impl SystemSampler {
    pub fn new() -> Self {
        Self {
            prev_cpus: read_cpu_samples(),
            prev_disks: read_disk_samples(),
            prev_time: Instant::now(),
        }
    }

    pub fn sample(&mut self) -> SystemStats {
        let now = Instant::now();
        let dt = now.duration_since(self.prev_time).as_secs_f64();
        if dt < 0.1 {
            return SystemStats::default();
        }

        let curr_cpus = read_cpu_samples();
        let cpus = curr_cpus
            .iter()
            .enumerate()
            .map(|(i, curr)| {
                self.prev_cpus
                    .get(i)
                    .map(|prev| {
                        let delta = curr.total().saturating_sub(prev.total()) as f64;
                        if delta > 0.0 {
                            CpuUsage {
                                user_pct: (curr.user.saturating_sub(prev.user) as f64 / delta
                                    * 100.0)
                                    as f32,
                                system_pct: ((curr.system + curr.irq + curr.softirq)
                                    .saturating_sub(prev.system + prev.irq + prev.softirq)
                                    as f64
                                    / delta
                                    * 100.0)
                                    as f32,
                                nice_pct: (curr.nice.saturating_sub(prev.nice) as f64 / delta
                                    * 100.0)
                                    as f32,
                                iowait_pct: (curr.iowait.saturating_sub(prev.iowait) as f64
                                    / delta
                                    * 100.0)
                                    as f32,
                            }
                        } else {
                            CpuUsage::default()
                        }
                    })
                    .unwrap_or_default()
            })
            .collect();

        let mem = read_mem();

        let curr_disks = read_disk_samples();
        let mut disks: Vec<DiskIO> = curr_disks
            .iter()
            .filter_map(|(name, curr)| {
                self.prev_disks.get(name).map(|prev| DiskIO {
                    name: name.clone(),
                    read_per_sec: (curr.sectors_read.saturating_sub(prev.sectors_read) as f64
                        * 512.0
                        / dt) as u64,
                    write_per_sec: (curr
                        .sectors_written
                        .saturating_sub(prev.sectors_written)
                        as f64
                        * 512.0
                        / dt) as u64,
                })
            })
            .collect();
        disks.sort_by(|a, b| a.name.cmp(&b.name));

        self.prev_cpus = curr_cpus;
        self.prev_disks = curr_disks;
        self.prev_time = now;

        SystemStats { cpus, mem, disks }
    }
}

fn read_cpu_samples() -> Vec<CpuSample> {
    let content = match std::fs::read_to_string("/proc/stat") {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };
    content
        .lines()
        .filter(|line| line.starts_with("cpu") && !line.starts_with("cpu "))
        .filter_map(|line| {
            let p: Vec<&str> = line.split_whitespace().collect();
            if p.len() >= 8 {
                Some(CpuSample {
                    user: p[1].parse().unwrap_or(0),
                    nice: p[2].parse().unwrap_or(0),
                    system: p[3].parse().unwrap_or(0),
                    idle: p[4].parse().unwrap_or(0),
                    iowait: p.get(5).and_then(|s| s.parse().ok()).unwrap_or(0),
                    irq: p.get(6).and_then(|s| s.parse().ok()).unwrap_or(0),
                    softirq: p.get(7).and_then(|s| s.parse().ok()).unwrap_or(0),
                    steal: p.get(8).and_then(|s| s.parse().ok()).unwrap_or(0),
                })
            } else {
                None
            }
        })
        .collect()
}

fn read_mem() -> MemUsage {
    let content = match std::fs::read_to_string("/proc/meminfo") {
        Ok(c) => c,
        Err(_) => return MemUsage::default(),
    };
    let mut m: HashMap<String, u64> = HashMap::new();
    for line in content.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 2 {
            let key = parts[0].trim_end_matches(':').to_string();
            // Values in /proc/meminfo are in kB
            let val: u64 = parts[1].parse().unwrap_or(0) * 1024;
            m.insert(key, val);
        }
    }
    let total = *m.get("MemTotal").unwrap_or(&0);
    let free = *m.get("MemFree").unwrap_or(&0);
    let buffers = *m.get("Buffers").unwrap_or(&0);
    let cached = *m.get("Cached").unwrap_or(&0);
    let sreclaimable = *m.get("SReclaimable").unwrap_or(&0);
    let swap_total = *m.get("SwapTotal").unwrap_or(&0);
    let swap_free = *m.get("SwapFree").unwrap_or(&0);
    MemUsage {
        total,
        used: total.saturating_sub(free + buffers + cached + sreclaimable),
        buffers,
        cached: cached + sreclaimable,
        swap_total,
        swap_used: swap_total.saturating_sub(swap_free),
    }
}

fn read_disk_samples() -> HashMap<String, DiskSample> {
    let content = match std::fs::read_to_string("/proc/diskstats") {
        Ok(c) => c,
        Err(_) => return HashMap::new(),
    };
    let mut samples = HashMap::new();
    for line in content.lines() {
        let p: Vec<&str> = line.split_whitespace().collect();
        if p.len() >= 10 {
            let name = p[2];
            if name.starts_with("loop") || name.starts_with("ram") || name.starts_with("dm-") {
                continue;
            }
            // Only whole block devices (not partitions)
            if !std::path::Path::new(&format!("/sys/block/{}", name)).exists() {
                continue;
            }
            let sr: u64 = p[5].parse().unwrap_or(0);
            let sw: u64 = p[9].parse().unwrap_or(0);
            if sr > 0 || sw > 0 {
                samples.insert(
                    name.to_string(),
                    DiskSample {
                        sectors_read: sr,
                        sectors_written: sw,
                    },
                );
            }
        }
    }
    samples
}
