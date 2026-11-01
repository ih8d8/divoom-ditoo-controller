use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub device_mac: Option<String>,

    #[serde(default = "default_duration")]
    pub default_duration: f64,

    #[serde(default = "default_brightness")]
    pub default_brightness: u8,

    #[serde(default)]
    pub quiet_hours: QuietHours,

    #[serde(default)]
    pub sound: SoundConfig,

    #[serde(default = "default_sync_interval")]
    pub sync_interval_mins: u64,

    #[serde(default)]
    pub desktop_monitor: DesktopMonitorConfig,

    #[serde(default)]
    pub sysmon: SysmonConfig,

    #[serde(default, alias = "device_battery")]
    pub laptop_battery: LaptopBatteryConfig,
}

fn default_duration() -> f64 {
    5.0
}

fn default_brightness() -> u8 {
    50
}

fn default_sync_interval() -> u64 {
    60
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuietHours {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_quiet_start")]
    pub start: String,
    #[serde(default = "default_quiet_end")]
    pub end: String,
}

impl Default for QuietHours {
    fn default() -> Self {
        Self {
            enabled: false,
            start: "22:00".to_string(),
            end: "07:00".to_string(),
        }
    }
}

fn default_quiet_start() -> String {
    "22:00".to_string()
}

fn default_quiet_end() -> String {
    "07:00".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SoundConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_sound_command")]
    pub command: String,
}

impl Default for SoundConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            command: "pw-play /usr/share/sounds/freedesktop/stereo/complete.oga".to_string(),
        }
    }
}

fn default_sound_command() -> String {
    "pw-play /usr/share/sounds/freedesktop/stereo/complete.oga".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DesktopMonitorConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_filter_apps")]
    pub filter_apps: Vec<String>,
}

impl Default for DesktopMonitorConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            filter_apps: default_filter_apps(),
        }
    }
}

fn default_filter_apps() -> Vec<String> {
    vec![
        "claude".to_string(),
        "codex".to_string(),
        "antigravity".to_string(),
        "cargo".to_string(),
        "terminal".to_string(),
        "kitty".to_string(),
        "alacritty".to_string(),
        "foot".to_string(),
    ]
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SysmonConfig {
    #[serde(default = "default_sysmon_enabled")]
    pub enabled: bool,
    #[serde(default = "default_sysmon_interval")]
    pub interval_secs: u64,
    #[serde(default = "default_cpu_threshold")]
    pub cpu_threshold: f32,
    #[serde(default = "default_ram_threshold")]
    pub ram_threshold: f32,
    #[serde(default = "default_disk_threshold")]
    pub disk_threshold: f32,
    #[serde(default = "default_disk_mount")]
    pub disk_mount: String,
    #[serde(default = "default_sysmon_cooldown")]
    pub cooldown_secs: u64,
    #[serde(default = "default_sysmon_alert_duration")]
    pub alert_duration: f64,
}

impl Default for SysmonConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            interval_secs: default_sysmon_interval(),
            cpu_threshold: default_cpu_threshold(),
            ram_threshold: default_ram_threshold(),
            disk_threshold: default_disk_threshold(),
            disk_mount: default_disk_mount(),
            cooldown_secs: default_sysmon_cooldown(),
            alert_duration: default_sysmon_alert_duration(),
        }
    }
}

fn default_sysmon_enabled() -> bool {
    true
}

fn default_sysmon_interval() -> u64 {
    15
}

fn default_cpu_threshold() -> f32 {
    75.0
}

fn default_ram_threshold() -> f32 {
    80.0
}

fn default_disk_threshold() -> f32 {
    85.0
}

fn default_disk_mount() -> String {
    "/".to_string()
}

fn default_sysmon_cooldown() -> u64 {
    120
}

fn default_sysmon_alert_duration() -> f64 {
    5.0
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LaptopBatteryConfig {
    #[serde(default = "default_low_threshold")]
    pub low_threshold: u8,
    #[serde(default = "default_critical_threshold")]
    pub critical_threshold: u8,
    #[serde(default = "default_warn_cooldown")]
    pub warn_cooldown_mins: u64,
}

pub type DeviceBatteryConfig = LaptopBatteryConfig;

impl Default for LaptopBatteryConfig {
    fn default() -> Self {
        Self {
            low_threshold: default_low_threshold(),
            critical_threshold: default_critical_threshold(),
            warn_cooldown_mins: default_warn_cooldown(),
        }
    }
}

fn default_low_threshold() -> u8 {
    20
}

fn default_critical_threshold() -> u8 {
    10
}

fn default_warn_cooldown() -> u64 {
    20
}

impl Default for Config {
    fn default() -> Self {
        Self {
            device_mac: None,
            default_duration: default_duration(),
            default_brightness: default_brightness(),
            quiet_hours: QuietHours::default(),
            sound: SoundConfig::default(),
            sync_interval_mins: default_sync_interval(),
            desktop_monitor: DesktopMonitorConfig::default(),
            sysmon: SysmonConfig::default(),
            laptop_battery: LaptopBatteryConfig::default(),
        }
    }
}

/// Resolves user config directory adhering strictly to XDG specification
pub fn resolve_config_dir() -> PathBuf {
    if let Some(dir) = dirs::config_dir() {
        dir
    } else if let Ok(home) = std::env::var("HOME") {
        PathBuf::from(home).join(".config")
    } else {
        PathBuf::from(".config")
    }
}

/// Resolves user data directory adhering strictly to XDG specification
pub fn resolve_data_dir() -> PathBuf {
    if let Some(dir) = dirs::data_dir() {
        dir
    } else if let Ok(home) = std::env::var("HOME") {
        PathBuf::from(home).join(".local").join("share")
    } else {
        PathBuf::from(".local/share")
    }
}

impl Config {
    pub fn config_path() -> PathBuf {
        resolve_config_dir().join("ditoo").join("config.json")
    }

    pub fn load() -> Self {
        let path = Self::config_path();
        if path.exists()
            && let Ok(content) = fs::read_to_string(&path)
            && let Ok(cfg) = serde_json::from_str(&content)
        {
            return cfg;
        }
        Config::default()
    }

    pub fn save(&self) -> std::io::Result<()> {
        let path = Self::config_path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(self)?;
        fs::write(path, json)?;
        Ok(())
    }
}
