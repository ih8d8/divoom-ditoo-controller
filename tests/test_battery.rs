use divoom_ditoo_controller::battery::{self, HostBattery, read_battery_from_sysfs};
use std::fs;
use std::path::{Path, PathBuf};

struct TestDir(PathBuf);

impl TestDir {
    fn new(prefix: &str) -> Self {
        let dir = std::env::temp_dir().join(format!(
            "{prefix}_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn test_host_battery_low_logic() {
    let mut bat = HostBattery::new(18, false, false, "Discharging");
    assert!(bat.is_low(), "18% discharging should be low");

    bat.is_charging = true;
    assert!(!bat.is_low(), "18% charging should not trigger low warning");

    bat.percent = 25;
    bat.is_charging = false;
    assert!(!bat.is_low(), "25% not charging should not be low");
}

#[test]
fn test_host_battery_critical_and_clock_override() {
    // Rule: < 10% and not charging must override clock
    let mut bat = HostBattery::new(8, false, false, "Discharging");
    assert!(bat.is_critical(), "8% not charging should be critical");
    assert!(
        bat.should_override_clock(),
        "8% not charging should override clock with battery icon"
    );

    // Rule: If charging, even if < 10%, it should NOT override clock
    bat.is_charging = true;
    assert!(!bat.is_critical(), "8% charging should not be critical");
    assert!(
        !bat.should_override_clock(),
        "8% charging MUST NOT override clock"
    );

    // Rule: >= 10% should not override clock
    bat.percent = 15;
    bat.is_charging = false;
    assert!(!bat.is_critical(), "15% not charging is not critical");
    assert!(
        !bat.should_override_clock(),
        "15% not charging should not override clock"
    );
}

#[test]
fn test_host_battery_threshold_logic() {
    let bat = HostBattery::new(25, false, false, "Discharging");
    assert!(
        bat.is_low_with_threshold(30),
        "25% should be low when threshold is 30"
    );
    assert!(
        !bat.is_low_with_threshold(20),
        "25% should not be low when threshold is 20"
    );
    assert!(
        bat.is_critical_with_threshold(30),
        "25% should be critical when threshold is 30"
    );
    assert!(
        !bat.is_critical_with_threshold(15),
        "25% should not be critical when threshold is 15"
    );
    assert!(bat.should_override_clock_with_threshold(30));
    assert!(!bat.should_override_clock_with_threshold(15));
}

#[test]
fn test_sysfs_parsing_discharging() {
    let dir = TestDir::new("test_bat_discharging");
    let bat_dir = dir.path().join("BAT0");
    fs::create_dir_all(&bat_dir).unwrap();

    fs::write(bat_dir.join("capacity"), "42\n").unwrap();
    fs::write(bat_dir.join("status"), "Discharging\n").unwrap();

    let bat = read_battery_from_sysfs(dir.path()).expect("Should parse BAT0");
    assert_eq!(bat.percent, 42);
    assert!(!bat.is_charging);
    assert!(!bat.ac_online);
    assert_eq!(bat.status, "Discharging");
}

#[test]
fn test_sysfs_parsing_charging_with_ac() {
    let dir = TestDir::new("test_bat_charging");
    let bat_dir = dir.path().join("BAT0");
    let adp_dir = dir.path().join("ADP0");
    fs::create_dir_all(&bat_dir).unwrap();
    fs::create_dir_all(&adp_dir).unwrap();

    fs::write(bat_dir.join("capacity"), "85\n").unwrap();
    fs::write(bat_dir.join("status"), "Charging\n").unwrap();
    fs::write(adp_dir.join("online"), "1\n").unwrap();

    let bat = read_battery_from_sysfs(dir.path()).expect("Should parse BAT0");
    assert_eq!(bat.percent, 85);
    assert!(bat.is_charging);
    assert!(bat.ac_online);
    assert_eq!(bat.status, "Charging");
}

#[test]
fn test_sysfs_parsing_ac_plugged_not_charging() {
    let dir = TestDir::new("test_bat_ac_not_charging");
    let bat_dir = dir.path().join("BAT1");
    let ac_dir = dir.path().join("ACAD");
    fs::create_dir_all(&bat_dir).unwrap();
    fs::create_dir_all(&ac_dir).unwrap();

    fs::write(bat_dir.join("capacity"), "60\n").unwrap();
    fs::write(bat_dir.join("status"), "Not charging\n").unwrap();
    fs::write(ac_dir.join("online"), "1\n").unwrap();

    let bat = read_battery_from_sysfs(dir.path()).expect("Should parse BAT1");
    assert_eq!(bat.percent, 60);
    // When AC is online and status is "Not charging" (e.g. threshold holding charge), treated as connected/charging
    assert!(bat.is_charging);
    assert!(bat.ac_online);
}

#[test]
fn test_sysfs_parsing_no_battery_desktop() {
    let dir = TestDir::new("test_bat_none");
    // Empty power_supply dir (desktop PC)
    let bat = read_battery_from_sysfs(dir.path());
    assert!(bat.is_none(), "Desktop without battery should return None");
}

#[test]
fn test_live_host_battery_call_does_not_panic() {
    // Tests get_host_battery() on the live system running tests
    let _ = battery::get_host_battery();
}
