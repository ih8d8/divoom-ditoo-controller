use std::fs;
use std::path::PathBuf;
use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};

/// State of the Divoom Ditoo Pro device battery
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceBatteryState {
    /// Battery level in percent (0..=100)
    pub percent: u8,
    /// Whether the Ditoo Pro is actively plugged into USB / charging
    pub is_charging: bool,
    /// Timestamp when this state was last recorded or updated
    pub updated_at: DateTime<Local>,
}

impl Default for DeviceBatteryState {
    fn default() -> Self {
        Self {
            percent: 85,
            is_charging: false,
            updated_at: Local::now(),
        }
    }
}

impl DeviceBatteryState {
    #[allow(dead_code)]
    pub fn new(percent: u8, is_charging: bool) -> Self {
        Self {
            percent: percent.min(100),
            is_charging,
            updated_at: Local::now(),
        }
    }

    /// Low battery warning threshold: <= 20% and not charging
    pub fn is_low(&self) -> bool {
        self.percent <= 20 && !self.is_charging
    }

    /// Critical battery threshold: < 10% and not charging
    pub fn is_critical(&self) -> bool {
        self.percent < 10 && !self.is_charging
    }

    /// Whether the device battery state requires overriding the clock with the battery icon
    /// Rules:
    /// - If device is charging: NEVER override clock (even if < 10%)
    /// - If device is NOT charging and < 10%: MUST override clock with battery icon so user is prompted to charge
    pub fn should_override_clock(&self) -> bool {
        self.is_critical()
    }

    pub fn description(&self) -> String {
        let state_str = if self.is_charging {
            "Charging (⚡)"
        } else if self.is_critical() {
            "CRITICAL (<10% - Battery icon replaces clock)"
        } else if self.is_low() {
            "Low warning (<=20%)"
        } else {
            "Normal"
        };
        format!("Ditoo Pro Battery: {}% [{}]", self.percent, state_str)
    }

    pub fn file_path() -> PathBuf {
        crate::config::resolve_config_dir().join("ditoo").join("device_battery.json")
    }

    pub fn load() -> Self {
        let path = Self::file_path();
        if path.exists()
            && let Ok(content) = fs::read_to_string(&path)
                && let Ok(mut state) = serde_json::from_str::<DeviceBatteryState>(&content) {
                    state.percent = state.percent.min(100);
                    return state;
                }
        Self::default()
    }

    pub fn save(&self) -> std::io::Result<()> {
        let path = Self::file_path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(self)?;
        fs::write(path, json)
    }

    /// Advance battery state over elapsed time:
    /// - If charging: charges at ~20% per hour (up to 100%)
    /// - If discharging: discharges at ~10% per hour at standard brightness (down to 0%)
    pub fn advance_time(&mut self, now: DateTime<Local>) {
        let elapsed_secs = (now - self.updated_at).num_seconds();
        if elapsed_secs < 30 {
            return;
        }

        let hours = elapsed_secs as f64 / 3600.0;
        if self.is_charging {
            let gained = (hours * 20.0).round() as u8;
            self.percent = (self.percent + gained).min(100);
        } else {
            let lost = (hours * 10.0).round() as u8;
            self.percent = self.percent.saturating_sub(lost);
        }
        self.updated_at = now;
        let _ = self.save();
    }
}

/// Query BlueZ D-Bus interface (org.bluez.Battery1) if exposed by Bluetooth stack
pub fn query_bluez_battery(mac: &str) -> Option<u8> {
    let dev_id = mac.replace(':', "_");
    let path = format!("/org/bluez/hci0/dev_{dev_id}");

    let output = std::process::Command::new("busctl")
        .args(["get-property", "org.bluez", &path, "org.bluez.Battery1", "Percentage"])
        .output()
        .ok()?;

    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let parts: Vec<&str> = stdout.split_whitespace().collect();
        if parts.len() >= 2
            && let Ok(pct) = parts[1].parse::<u8>() {
                return Some(pct.min(100));
            }
    }
    None
}

/// Optional helper for Linux host laptop battery (/sys/class/power_supply)
/// Explicitly distinct from Ditoo Pro device battery.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct HostBatteryInfo {
    pub capacity: u8,
    pub status: String,
    pub is_charging: bool,
}

#[allow(dead_code)]
pub fn get_host_laptop_battery() -> Option<HostBatteryInfo> {
    let base = PathBuf::from("/sys/class/power_supply");
    if !base.exists() {
        return None;
    }

    let entries = fs::read_dir(base).ok()?;
    for entry in entries.flatten() {
        let file_name = entry.file_name().to_string_lossy().to_string();
        if file_name.starts_with("BAT") {
            let cap_path = entry.path().join("capacity");
            let stat_path = entry.path().join("status");

            if let Ok(cap_str) = fs::read_to_string(&cap_path)
                && let Ok(capacity) = cap_str.trim().parse::<u8>() {
                    let status = fs::read_to_string(&stat_path)
                        .unwrap_or_else(|_| "Unknown".into())
                        .trim()
                        .to_string();
                    let is_charging = status.eq_ignore_ascii_case("charging");
                    return Some(HostBatteryInfo {
                        capacity,
                        status,
                        is_charging,
                    });
                }
        }
    }

    None
}
