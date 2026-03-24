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
pub struct ProcessStats {
    pub found: bool,
    pub cpu_pct: f32,
    pub rss: u64, // bytes
}

#[derive(Clone, Default)]
pub struct SystemStats {
    pub cpus: Vec<CpuUsage>,
    pub mem: MemUsage,
    pub disks: Vec<DiskIO>,
    pub bitcoind: ProcessStats,
    pub tor: ProcessStats,
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

struct ProcSample {
    pid: u32,
    utime: u64,
    stime: u64,
}

pub struct SystemSampler {
    prev_cpus: Vec<CpuSample>,
    prev_disks: HashMap<String, DiskSample>,
    prev_bitcoind: Option<ProcSample>,
    prev_tor: Option<ProcSample>,
    prev_time: Instant,
}

impl SystemSampler {
    pub fn new() -> Self {
        let prev_bitcoind = find_pid_by_name(&["bitcoin", "knots"]).and_then(read_proc_cpu);
        let prev_tor = find_pid_by_name(&["tor"]).and_then(read_proc_cpu);
        Self {
            prev_cpus: read_cpu_samples(),
            prev_disks: read_disk_samples(),
            prev_bitcoind,
            prev_tor,
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

        let bitcoind = sample_process(&self.prev_bitcoind, &["bitcoin", "knots"], dt);
        self.prev_bitcoind = find_pid_by_name(&["bitcoin", "knots"]).and_then(read_proc_cpu);

        let tor = sample_process(&self.prev_tor, &["tor"], dt);
        self.prev_tor = find_pid_by_name(&["tor"]).and_then(read_proc_cpu);

        self.prev_cpus = curr_cpus;
        self.prev_disks = curr_disks;
        self.prev_time = now;

        SystemStats { cpus, mem, disks, bitcoind, tor }
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

/// Sample a tracked process: compute CPU% from previous ticks, read current RSS
fn sample_process(prev: &Option<ProcSample>, names: &[&str], dt: f64) -> ProcessStats {
    let pid = prev.as_ref().map(|p| p.pid).or_else(|| find_pid_by_name(names));
    let curr = pid.and_then(read_proc_cpu);
    match (prev, &curr) {
        (Some(prev), Some(curr)) if prev.pid == curr.pid => {
            let clk_tck = 100.0;
            let d_utime = curr.utime.saturating_sub(prev.utime) as f64;
            let d_stime = curr.stime.saturating_sub(prev.stime) as f64;
            let cpu_pct = ((d_utime + d_stime) / clk_tck / dt * 100.0) as f32;
            ProcessStats { found: true, cpu_pct, rss: read_proc_rss(curr.pid) }
        }
        (None, Some(curr)) => {
            ProcessStats { found: true, cpu_pct: 0.0, rss: read_proc_rss(curr.pid) }
        }
        _ => ProcessStats::default(),
    }
}

/// Find a PID by matching process comm or cmdline against a list of name prefixes
fn find_pid_by_name(names: &[&str]) -> Option<u32> {
    let proc_dir = match std::fs::read_dir("/proc") {
        Ok(d) => d,
        Err(_) => return None,
    };
    for entry in proc_dir.flatten() {
        let name = entry.file_name();
        let name_str = name.to_str().unwrap_or("");
        if name_str.chars().all(|c| c.is_ascii_digit()) {
            let pid: u32 = match name_str.parse() {
                Ok(p) => p,
                Err(_) => continue,
            };
            let comm_path = entry.path().join("comm");
            if let Ok(comm) = std::fs::read_to_string(&comm_path) {
                let comm = comm.trim().to_lowercase();
                if names.iter().any(|n| comm.starts_with(n)) {
                    return Some(pid);
                }
            }
            let cmdline_path = entry.path().join("cmdline");
            if let Ok(cmdline) = std::fs::read_to_string(&cmdline_path) {
                let cmdline = cmdline.to_lowercase();
                if names.iter().any(|n| cmdline.contains(n)) {
                    return Some(pid);
                }
            }
        }
    }
    None
}

/// Read utime + stime from /proc/[pid]/stat (fields 14 and 15, 1-indexed)
fn read_proc_cpu(pid: u32) -> Option<ProcSample> {
    let content = std::fs::read_to_string(format!("/proc/{}/stat", pid)).ok()?;
    // Fields after the comm (which is in parens and may contain spaces)
    let after_comm = content.rfind(')')? + 2;
    let fields: Vec<&str> = content[after_comm..].split_whitespace().collect();
    // fields[0] = state, fields[11] = utime (14th overall), fields[12] = stime (15th overall)
    if fields.len() < 13 { return None; }
    let utime: u64 = fields[11].parse().ok()?;
    let stime: u64 = fields[12].parse().ok()?;
    Some(ProcSample { pid, utime, stime })
}

/// Read RSS from /proc/[pid]/statm (field 2, in pages)
fn read_proc_rss(pid: u32) -> u64 {
    let content = match std::fs::read_to_string(format!("/proc/{}/statm", pid)) {
        Ok(c) => c,
        Err(_) => return 0,
    };
    let fields: Vec<&str> = content.split_whitespace().collect();
    if fields.len() >= 2 {
        let pages: u64 = fields[1].parse().unwrap_or(0);
        pages * 4096 // page size
    } else {
        0
    }
}
