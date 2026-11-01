use divoom_ditoo_controller::sysmon::{CpuSample, DiskInfo, MemInfo};

#[test]
fn test_cpu_usage_calculation() {
    let s1 = CpuSample { total: 1000, idle: 800 };
    let s2 = CpuSample { total: 1200, idle: 900 };

    // delta_total = 200, delta_idle = 100, busy = 100 -> 50.0%
    let usage = s2.usage_since(&s1);
    assert!((usage - 50.0).abs() < 0.01, "Expected 50.0%, got {}", usage);

    // Identical samples -> 0.0%
    assert_eq!(s1.usage_since(&s1), 0.0);

    // All idle delta -> 0.0%
    let s3 = CpuSample { total: 1100, idle: 900 };
    assert_eq!(s3.usage_since(&s1), 0.0);

    // All busy delta -> 100.0%
    let s4 = CpuSample { total: 1100, idle: 800 };
    assert_eq!(s4.usage_since(&s1), 100.0);
}

#[test]
fn test_meminfo_reader() {
    if let Some(mem) = MemInfo::read() {
        assert!(mem.total_kb > 0, "MemTotal should be greater than 0");
        assert!(mem.available_kb <= mem.total_kb, "MemAvailable should be <= MemTotal");
        assert!(mem.percent_used >= 0.0 && mem.percent_used <= 100.0, "Percent used should be 0..=100");
    }
}

#[test]
fn test_disk_info_reader() {
    if let Some(disk) = DiskInfo::read_mount("/") {
        assert!(disk.total_bytes > 0, "Total bytes should be greater than 0");
        assert!(disk.available_bytes <= disk.total_bytes, "Available bytes should be <= Total bytes");
        assert!(disk.percent_used >= 0.0 && disk.percent_used <= 100.0, "Disk percent used should be 0..=100");
    }
}
