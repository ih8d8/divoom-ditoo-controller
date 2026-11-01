use divoom_ditoo_controller::canvas::Canvas;
use std::path::Path;

fn main() {
    let _ = std::fs::create_dir_all("target/screens");

    // 1. Sysmon (CPU 42%, RAM 58%)
    let sysmon = Canvas::render_sysmon(42, 58);
    sysmon
        .save_png(Path::new("target/screens/sysmon.png"))
        .unwrap();

    // 2. Battery (85%, Charging)
    let battery = Canvas::render_battery(85, true, false);
    battery
        .save_png(Path::new("target/screens/battery.png"))
        .unwrap();

    // 3. Disk Warning (88%)
    let disk = Canvas::render_gauge("DISK", 88, 100, [239, 68, 68]);
    disk.save_png(Path::new("target/screens/disk_warning.png"))
        .unwrap();

    // 4. Authentic Native Ditoo Clock face: matching physical Ditoo Pro hardware
    // Hours (20) on top; blinking colon on bottom-left, minutes (20) on bottom-right
    let mut clock = Canvas::new();
    let emerald = [34, 197, 94];

    let pattern = [
        "................", // 0
        "....####...####.", // 1: Hours "20" (shifted right by 1 pixel to align with minutes)
        "...#....#.#....#", // 2
        "........#.#....#", // 3
        "....####..#....#", // 4
        "...#......#....#", // 5
        "...#......#....#", // 6
        "...######..####.", // 7
        "................", // 8: Row separator
        "....####...####.", // 9: Minutes ": 20"
        ".#.#....#.#....#", // 10: Colon top dot at (1, 10)
        "........#.#....#", // 11
        "....####..#....#", // 12
        ".#.#......#....#", // 13: Colon bottom dot at (1, 13)
        "...#......#....#", // 14
        "...######..####.", // 15
    ];

    for (y, row) in pattern.iter().enumerate() {
        for (x, ch) in row.chars().enumerate() {
            if ch == '#' {
                clock.set_pixel(x, y, emerald);
            }
        }
    }

    clock
        .save_png(Path::new("target/screens/clock.png"))
        .unwrap();

    println!("Saved 16x16 raw screens to target/screens/");
}
