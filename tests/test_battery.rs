use chrono::Local;
use divoom_ditoo_controller::battery::DeviceBatteryState;

#[test]
fn test_battery_low_logic() {
    let mut state = DeviceBatteryState::new(18, false);
    assert!(state.is_low(), "18% not charging should be low");

    state.is_charging = true;
    assert!(!state.is_low(), "18% charging should not trigger low warning");

    state.percent = 25;
    state.is_charging = false;
    assert!(!state.is_low(), "25% not charging should not be low");
}

#[test]
fn test_battery_critical_and_clock_override() {
    // Rule: < 10% and not charging must override clock
    let mut state = DeviceBatteryState::new(8, false);
    assert!(state.is_critical(), "8% not charging should be critical");
    assert!(state.should_override_clock(), "8% not charging should override clock with battery icon");

    // Rule: If charging, even if < 10%, it should NOT override clock
    state.is_charging = true;
    assert!(!state.is_critical(), "8% charging should not be critical");
    assert!(!state.should_override_clock(), "8% charging MUST NOT override clock");

    // Rule: >= 10% should not override clock
    state.percent = 15;
    state.is_charging = false;
    assert!(!state.is_critical(), "15% not charging is not critical");
    assert!(!state.should_override_clock(), "15% not charging should not override clock");
}

#[test]
fn test_battery_advance_time() {
    let mut state = DeviceBatteryState::new(50, false);
    let now = Local::now();
    state.updated_at = now - chrono::Duration::hours(2);

    // After 2 hours discharging at ~10%/hr -> ~30%
    state.advance_time(now);
    assert!(state.percent <= 35 && state.percent >= 25, "Expected ~30% after 2 hours discharge, got {}", state.percent);

    // After 2 hours charging at ~20%/hr -> ~70%
    state.is_charging = true;
    state.updated_at = now - chrono::Duration::hours(2);
    state.advance_time(now);
    assert!(state.percent >= 65 && state.percent <= 75, "Expected ~70% after 2 hours charge, got {}", state.percent);
}
