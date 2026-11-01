use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

/// Host laptop battery information read from Linux sysfs (/sys/class/power_supply)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HostBattery {
    /// Battery level in percent (0..=100)
    pub percent: u8,
    /// Whether the laptop is actively charging or connected to AC power
    pub is_charging: bool,
    /// Whether external AC power adapter is connected
    pub ac_online: bool,
    /// Raw sysfs status string ("Charging", "Discharging", "Full", "Not charging", etc.)
    pub status: String,
}

pub type HostBatteryInfo = HostBattery;

impl HostBattery {
    pub fn new(percent: u8, is_charging: bool, ac_online: bool, status: &str) -> Self {
        Self {
            percent: percent.min(100),
            is_charging,
            ac_online,
            status: status.to_string(),
        }
    }

    /// Low battery warning threshold: <= 20% and not charging
    pub fn is_low(&self) -> bool {
        self.is_low_with_threshold(20)
    }

    pub fn is_low_with_threshold(&self, threshold: u8) -> bool {
        self.percent <= threshold && !self.is_charging
    }

    /// Critical battery threshold: < 10% and not charging
    pub fn is_critical(&self) -> bool {
        self.is_critical_with_threshold(10)
    }

    pub fn is_critical_with_threshold(&self, threshold: u8) -> bool {
        self.percent < threshold && !self.is_charging
    }

    /// Whether the battery state requires overriding the clock with the battery icon
    /// Rules:
    /// - If charging / AC plugged in: NEVER override clock (even if < 10%)
    /// - If NOT charging and < 10%: MUST override clock with battery icon so user is prompted to plug in
    pub fn should_override_clock(&self) -> bool {
        self.is_critical()
    }

    pub fn should_override_clock_with_threshold(&self, threshold: u8) -> bool {
        self.is_critical_with_threshold(threshold)
    }

    pub fn description(&self) -> String {
        let chg_str = if self.is_charging {
            " [Charging ⚡]"
        } else if self.ac_online {
            " [AC Connected 🔌]"
        } else if self.is_critical() {
            " [CRITICAL <10% - Battery alert active]"
        } else if self.is_low() {
            " [Low warning <=20%]"
        } else {
            " [Discharging]"
        };
        format!("Laptop Battery: {}%{}", self.percent, chg_str)
    }
}

/// Detect and read host laptop battery from Linux sysfs (/sys/class/power_supply).
/// Returns `None` if running on a desktop PC or system without a battery.
pub fn get_host_battery() -> Option<HostBattery> {
    read_battery_from_sysfs(Path::new("/sys/class/power_supply"))
}

pub fn get_host_laptop_battery() -> Option<HostBattery> {
    get_host_battery()
}

/// Testable sysfs battery reader
pub fn read_battery_from_sysfs(base: &Path) -> Option<HostBattery> {
    if !base.exists() {
        return None;
    }

    let ac_online = check_ac_online(base);

    let entries = fs::read_dir(base).ok()?;
    for entry in entries.flatten() {
        let file_name = entry.file_name().to_string_lossy().to_string();
        if file_name.starts_with("BAT") {
            let cap_path = entry.path().join("capacity");
            let stat_path = entry.path().join("status");

            if let Ok(cap_str) = fs::read_to_string(&cap_path)
                && let Ok(capacity) = cap_str.trim().parse::<u8>()
            {
                let status = fs::read_to_string(&stat_path)
                    .unwrap_or_else(|_| "Unknown".into())
                    .trim()
                    .to_string();
                let is_charging = status.eq_ignore_ascii_case("charging")
                    || (ac_online && !status.eq_ignore_ascii_case("discharging"));

                return Some(HostBattery {
                    percent: capacity.min(100),
                    is_charging,
                    ac_online,
                    status,
                });
            }
        }
    }

    None
}

/// Helper to check if any AC power adapter is online
fn check_ac_online(base: &Path) -> bool {
    if let Ok(entries) = fs::read_dir(base) {
        for entry in entries.flatten() {
            let file_name = entry.file_name().to_string_lossy().to_string();
            if file_name.starts_with("AC") || file_name.starts_with("ADP") {
                let online_path = entry.path().join("online");
                if let Ok(online_str) = fs::read_to_string(&online_path)
                    && online_str.trim() == "1"
                {
                    return true;
                }
            }
        }
    }
    false
}
