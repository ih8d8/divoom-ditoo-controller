use std::ffi::CString;
use std::fs;
use std::mem::MaybeUninit;
use serde::{Deserialize, Serialize};

/// Point-in-time snapshot of CPU tick counters from /proc/stat
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CpuSample {
    pub total: u64,
    pub idle: u64,
}

impl CpuSample {
    /// Read CPU counters from Linux /proc/stat
    pub fn read() -> Option<Self> {
        let content = fs::read_to_string("/proc/stat").ok()?;
        let first_line = content.lines().next()?;
        if !first_line.starts_with("cpu ") {
            return None;
        }

        let fields: Vec<u64> = first_line
            .split_whitespace()
            .skip(1)
            .filter_map(|s| s.parse().ok())
            .collect();

        if fields.len() < 4 {
            return None;
        }

        // fields: [user, nice, system, idle, iowait, irq, softirq, steal, guest, guest_nice]
        let idle = fields[3] + fields.get(4).copied().unwrap_or(0);
        let total: u64 = fields.iter().sum();

        Some(CpuSample { total, idle })
    }

    /// Calculate CPU usage percentage since a previous sample
    pub fn usage_since(&self, previous: &CpuSample) -> f32 {
        let delta_total = self.total.saturating_sub(previous.total);
        let delta_idle = self.idle.saturating_sub(previous.idle);

        if delta_total == 0 {
            return 0.0;
        }

        let busy = delta_total.saturating_sub(delta_idle);
        let usage = (busy as f64 / delta_total as f64) * 100.0;
        (usage as f32).clamp(0.0, 100.0)
    }
}

/// System RAM memory statistics from /proc/meminfo
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct MemInfo {
    pub total_kb: u64,
    pub available_kb: u64,
    pub used_kb: u64,
    pub percent_used: f32,
}

impl MemInfo {
    /// Read RAM metrics from Linux /proc/meminfo
    pub fn read() -> Option<Self> {
        let content = fs::read_to_string("/proc/meminfo").ok()?;
        let mut total: Option<u64> = None;
        let mut available: Option<u64> = None;

        for line in content.lines() {
            if line.starts_with("MemTotal:") {
                total = line.split_whitespace().nth(1).and_then(|s| s.parse().ok());
            } else if line.starts_with("MemAvailable:") {
                available = line.split_whitespace().nth(1).and_then(|s| s.parse().ok());
            }
            if total.is_some() && available.is_some() {
                break;
            }
        }

        let total_kb = total?;
        let available_kb = available?;
        let used_kb = total_kb.saturating_sub(available_kb);
        let percent = if total_kb > 0 {
            ((used_kb as f64 / total_kb as f64) * 100.0) as f32
        } else {
            0.0
        };

        Some(MemInfo {
            total_kb,
            available_kb,
            used_kb,
            percent_used: percent.clamp(0.0, 100.0),
        })
    }
}

/// Disk filesystem statistics via POSIX statvfs
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq)]
pub struct DiskInfo {
    pub total_bytes: u64,
    pub available_bytes: u64,
    pub used_bytes: u64,
    pub percent_used: f32,
}

impl DiskInfo {
    /// Read filesystem metrics for a mount path (default: "/")
    pub fn read_mount(path: &str) -> Option<Self> {
        let c_path = CString::new(path).ok()?;
        let mut stat = MaybeUninit::<libc::statvfs>::uninit();
        let res = unsafe { libc::statvfs(c_path.as_ptr(), stat.as_mut_ptr()) };
        if res != 0 {
            return None;
        }
        let stat = unsafe { stat.assume_init() };

        let block_size = if stat.f_frsize > 0 {
            stat.f_frsize
        } else {
            stat.f_bsize
        };

        let total_blocks = stat.f_blocks;
        let avail_blocks = stat.f_bavail;

        let total_bytes = total_blocks.saturating_mul(block_size);
        let available_bytes = avail_blocks.saturating_mul(block_size);
        let used_bytes = total_bytes.saturating_sub(available_bytes);

        let percent = if total_bytes > 0 {
            ((used_bytes as f64 / total_bytes as f64) * 100.0) as f32
        } else {
            0.0
        };

        Some(DiskInfo {
            total_bytes,
            available_bytes,
            used_bytes,
            percent_used: percent.clamp(0.0, 100.0),
        })
    }
}

/// Combined snapshot of system CPU, RAM, and Disk usage
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SysmonSnapshot {
    pub cpu_pct: f32,
    pub ram_pct: f32,
    pub ram_used_mb: u64,
    pub ram_total_mb: u64,
    pub disk_pct: f32,
    pub disk_used_gb: f64,
    pub disk_total_gb: f64,
}

impl SysmonSnapshot {
    #[allow(dead_code)]
    pub fn description(&self) -> String {
        format!(
            "CPU: {:.1}% | RAM: {:.1}% ({}/{} MB) | Disk: {:.1}% ({:.1}/{:.1} GB)",
            self.cpu_pct,
            self.ram_pct,
            self.ram_used_mb,
            self.ram_total_mb,
            self.disk_pct,
            self.disk_used_gb,
            self.disk_total_gb
        )
    }
}

/// Instantaneous reading of system CPU, RAM, and root filesystem stats
pub fn read_instant_snapshot() -> SysmonSnapshot {
    read_instant_snapshot_mount("/")
}

/// Instantaneous reading specifying custom mount point
pub fn read_instant_snapshot_mount(mount: &str) -> SysmonSnapshot {
    let s1 = CpuSample::read().unwrap_or_default();
    std::thread::sleep(std::time::Duration::from_millis(120));
    let s2 = CpuSample::read().unwrap_or_default();
    let cpu_pct = s2.usage_since(&s1);

    let mem = MemInfo::read().unwrap_or_default();
    let disk = DiskInfo::read_mount(mount).unwrap_or_default();

    SysmonSnapshot {
        cpu_pct,
        ram_pct: mem.percent_used,
        ram_used_mb: mem.used_kb / 1024,
        ram_total_mb: mem.total_kb / 1024,
        disk_pct: disk.percent_used,
        disk_used_gb: disk.used_bytes as f64 / (1024.0 * 1024.0 * 1024.0),
        disk_total_gb: disk.total_bytes as f64 / (1024.0 * 1024.0 * 1024.0),
    }
}
