use divoom_ditoo_controller::canvas::Canvas;

#[test]
fn test_canvas_text_measurement() {
    assert_eq!(Canvas::measure_text_3x5("5"), 3);
    // "59%": 3 + 1 + 3 + 1 + 3 = 11
    assert_eq!(Canvas::measure_text_3x5("59%"), 11);
    // "100%": 1 + 1 + 3 + 1 + 3 + 1 + 3 = 13
    assert_eq!(Canvas::measure_text_3x5("100%"), 13);
    // "LOW": 3 + 1 + 3 + 1 + 5 = 13
    assert_eq!(Canvas::measure_text_3x5("LOW"), 13);
}

#[test]
fn test_battery_canvas_generation() {
    let canvas = Canvas::render_battery(50, false, false);
    // Outer shell should have grey pixels at (1, 1)
    let p = canvas.get_pixel(1, 1);
    assert_eq!(p, [120, 120, 120]);

    // Inner fill should have green pixels at (2, 2)
    let fill = canvas.get_pixel(2, 2);
    assert_eq!(fill, [34, 197, 94]);
}

#[test]
fn test_low_battery_canvas_generation() {
    let canvas = Canvas::render_battery(15, false, true);
    // Frame and fill should be red
    let frame = canvas.get_pixel(1, 1);
    assert_eq!(frame, [220, 38, 38]);
    let fill = canvas.get_pixel(2, 2);
    assert_eq!(fill, [239, 68, 68]);
}

#[test]
fn test_charging_battery_canvas_generation() {
    let canvas = Canvas::render_battery(80, true, false);
    // Fill should be cyan
    let fill = canvas.get_pixel(2, 2);
    assert_eq!(fill, [0, 229, 255]);
    // Lightning bolt pixel at (7, 2) should be yellow
    let bolt = canvas.get_pixel(7, 2);
    assert_eq!(bolt, [255, 235, 59]);
}

#[test]
fn test_gauge_canvas_generation() {
    let canvas = Canvas::render_gauge("CPU", 50, 100, [0, 255, 0]);
    // Progress fill at (2, 10)
    assert_eq!(canvas.get_pixel(2, 10), [0, 255, 0]);
}

#[test]
fn test_hex_pixels_parsing() {
    let mut hex_data = String::new();
    for _ in 0..256 {
        hex_data.push_str("#FF0000 ");
    }
    let canvas = Canvas::from_hex_pixels(&hex_data).expect("Should parse 256 colors");
    assert_eq!(canvas.get_pixel(0, 0), [255, 0, 0]);
    assert_eq!(canvas.get_pixel(15, 15), [255, 0, 0]);
}

#[test]
fn test_sysmon_canvas_generation() {
    let canvas = Canvas::render_sysmon(78, 45);
    // Should have non-black pixels in CPU and RAM bars
    let cpu_bar = canvas.get_pixel(2, 6);
    assert_ne!(cpu_bar, [0, 0, 0], "CPU bar should be drawn at (2, 6)");

    let ram_bar = canvas.get_pixel(2, 13);
    assert_ne!(ram_bar, [0, 0, 0], "RAM bar should be drawn at (2, 13)");

    // Convert to temp PNG
    let tmp = canvas.to_temp_png().expect("Canvas should convert to temp PNG");
    assert!(tmp.exists(), "Temp PNG file should exist");
    let _ = std::fs::remove_file(tmp);
}
