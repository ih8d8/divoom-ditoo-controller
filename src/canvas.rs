use std::fs::OpenOptions;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use image::{ImageBuffer, Rgb};
use serde::{Deserialize, Serialize};

static TEMP_FILE_SEQ: AtomicU64 = AtomicU64::new(0);

/// Returns a secure, user-isolated directory for ephemeral render files.
/// Prioritizes $XDG_RUNTIME_DIR (which is already mode 0700 on tmpfs)
/// or falls back to /tmp/ditoo-<uid>/ with strict 0700 permissions.
pub fn get_secure_runtime_temp_dir() -> std::io::Result<PathBuf> {
    let dir = if let Ok(runtime_dir) = std::env::var("XDG_RUNTIME_DIR") {
        PathBuf::from(runtime_dir).join("ditoo")
    } else {
        let uid = unsafe { libc::getuid() };
        std::env::temp_dir().join(format!("ditoo-{}", uid))
    };

    if !dir.exists() {
        std::fs::create_dir_all(&dir)?;
        let mut perms = std::fs::metadata(&dir)?.permissions();
        perms.set_mode(0o700);
        let _ = std::fs::set_permissions(&dir, perms);
    }
    Ok(dir)
}

/// Creates a new temporary file with mode 0600 (owner read/write only)
/// using O_CREAT | O_EXCL to eliminate symlink attacks and race conditions.
pub fn create_secure_temp_file(prefix: &str, extension: &str) -> std::io::Result<(PathBuf, std::fs::File)> {
    let dir = get_secure_runtime_temp_dir()?;
    let pid = std::process::id();
    let mut attempts = 0;

    loop {
        let count = TEMP_FILE_SEQ.fetch_add(1, Ordering::Relaxed);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let path = dir.join(format!("{prefix}_{pid}_{nanos}_{count}.{extension}"));

        match OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&path)
        {
            Ok(file) => return Ok((path, file)),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists && attempts < 100 => {
                attempts += 1;
                continue;
            }
            Err(e) => return Err(e),
        }
    }
}

pub const CANVAS_WIDTH: usize = 16;
pub const CANVAS_HEIGHT: usize = 16;

pub fn parse_color(s: &str) -> Result<[u8; 3], String> {
    let c = csscolorparser::parse(s).map_err(|e| format!("Invalid color '{s}': {e}"))?;
    let [r, g, b, _] = c.to_rgba8();
    Ok([r, g, b])
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "widget", rename_all = "snake_case")]
pub enum CanvasType {
    Battery {
        percent: u8,
        #[serde(default)]
        charging: bool,
        #[serde(default)]
        is_low: bool,
    },
    Gauge {
        label: String,
        value: u32,
        max: u32,
        #[serde(default = "default_gauge_color")]
        color: String,
    },
    Card {
        top: String,
        bottom: String,
        #[serde(default = "default_card_top")]
        top_color: String,
        #[serde(default = "default_card_bottom")]
        bottom_color: String,
        #[serde(default = "default_card_bg")]
        bg_color: String,
    },
    Metric {
        label: String,
        value: String,
        #[serde(default = "default_metric_color")]
        color: String,
    },
    Pixels {
        hex_data: String,
    },
    Sysmon {
        cpu: u32,
        ram: u32,
    },
}

fn default_gauge_color() -> String {
    "green".to_string()
}

fn default_card_top() -> String {
    "white".to_string()
}

fn default_card_bottom() -> String {
    "green".to_string()
}

fn default_card_bg() -> String {
    "black".to_string()
}

fn default_metric_color() -> String {
    "cyan".to_string()
}

impl CanvasType {
    pub fn render(&self) -> Result<Canvas, String> {
        match self {
            CanvasType::Battery {
                percent,
                charging,
                is_low,
            } => Ok(Canvas::render_battery(*percent, *charging, *is_low)),
            CanvasType::Gauge {
                label,
                value,
                max,
                color,
            } => {
                let rgb = parse_color(color)?;
                Ok(Canvas::render_gauge(label, *value, *max, rgb))
            }
            CanvasType::Card {
                top,
                bottom,
                top_color,
                bottom_color,
                bg_color,
            } => {
                let fg_top = parse_color(top_color)?;
                let fg_bot = parse_color(bottom_color)?;
                let bg = parse_color(bg_color)?;
                Ok(Canvas::render_card(top, bottom, fg_top, fg_bot, bg))
            }
            CanvasType::Metric {
                label,
                value,
                color,
            } => {
                let rgb = parse_color(color)?;
                Ok(Canvas::render_metric(label, value, rgb))
            }
            CanvasType::Pixels { hex_data } => Canvas::from_hex_pixels(hex_data),
            CanvasType::Sysmon { cpu, ram } => Ok(Canvas::render_sysmon(*cpu, *ram)),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Canvas {
    pub pixels: [[u8; 3]; 256],
}

impl Default for Canvas {
    fn default() -> Self {
        Self::new()
    }
}

impl Canvas {
    pub fn new() -> Self {
        Self {
            pixels: [[0, 0, 0]; 256],
        }
    }

    pub fn with_bg(color: [u8; 3]) -> Self {
        Self {
            pixels: [color; 256],
        }
    }

    #[inline]
    pub fn set_pixel(&mut self, x: usize, y: usize, color: [u8; 3]) {
        if x < CANVAS_WIDTH && y < CANVAS_HEIGHT {
            self.pixels[y * CANVAS_WIDTH + x] = color;
        }
    }

    #[inline]
    pub fn get_pixel(&self, x: usize, y: usize) -> [u8; 3] {
        if x < CANVAS_WIDTH && y < CANVAS_HEIGHT {
            self.pixels[y * CANVAS_WIDTH + x]
        } else {
            [0, 0, 0]
        }
    }

    #[allow(dead_code)]
    pub fn fill(&mut self, color: [u8; 3]) {
        self.pixels.fill(color);
    }

    pub fn fill_rect(&mut self, x: usize, y: usize, w: usize, h: usize, color: [u8; 3]) {
        for dy in 0..h {
            for dx in 0..w {
                self.set_pixel(x + dx, y + dy, color);
            }
        }
    }

    pub fn draw_rect(&mut self, x: usize, y: usize, w: usize, h: usize, color: [u8; 3]) {
        if w == 0 || h == 0 {
            return;
        }
        for dx in 0..w {
            self.set_pixel(x + dx, y, color);
            self.set_pixel(x + dx, y + h - 1, color);
        }
        for dy in 0..h {
            self.set_pixel(x, y + dy, color);
            self.set_pixel(x + w - 1, y + dy, color);
        }
    }

    /// Measures rendered width of text in 3x5 font including 1px spacing
    pub fn measure_text_3x5(text: &str) -> usize {
        let mut width = 0;
        let mut count = 0;
        for c in text.chars() {
            let (glyph_w, _) = glyph_3x5(c);
            width += glyph_w;
            count += 1;
        }
        if count > 1 {
            width += count - 1; // 1px spacing
        }
        width
    }

    /// Draws a single character in 3x5 font. Returns glyph width.
    pub fn draw_char_3x5(&mut self, x: usize, y: usize, c: char, color: [u8; 3]) -> usize {
        let (width, rows) = glyph_3x5(c);
        for (row_idx, &row_bits) in rows.iter().enumerate() {
            for col_idx in 0..width {
                // Highest bit corresponds to leftmost pixel
                let shift = width - 1 - col_idx;
                if ((row_bits >> shift) & 1) == 1 {
                    self.set_pixel(x + col_idx, y + row_idx, color);
                }
            }
        }
        width
    }

    /// Draws text left-aligned at (x, y)
    pub fn draw_text_3x5(&mut self, mut x: usize, y: usize, text: &str, color: [u8; 3]) {
        for c in text.chars() {
            let w = self.draw_char_3x5(x, y, c, color);
            x += w + 1;
            if x >= CANVAS_WIDTH {
                break;
            }
        }
    }

    /// Draws text centered horizontally at row y
    pub fn draw_text_centered(&mut self, y: usize, text: &str, color: [u8; 3]) {
        let total_w = Self::measure_text_3x5(text);
        let x = if total_w < CANVAS_WIDTH {
            (CANVAS_WIDTH - total_w) / 2
        } else {
            0
        };
        self.draw_text_3x5(x, y, text, color);
    }

    /// Save to PNG file path
    pub fn save_png(&self, path: &Path) -> Result<(), String> {
        let mut img = ImageBuffer::<Rgb<u8>, Vec<u8>>::new(CANVAS_WIDTH as u32, CANVAS_HEIGHT as u32);
        for y in 0..CANVAS_HEIGHT {
            for x in 0..CANVAS_WIDTH {
                let [r, g, b] = self.get_pixel(x, y);
                img.put_pixel(x as u32, y as u32, Rgb([r, g, b]));
            }
        }
        img.save(path).map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Save to a secure temporary PNG file and return path
    pub fn to_temp_png(&self) -> Result<PathBuf, String> {
        let (temp_path, mut file) = create_secure_temp_file("ditoo_canvas", "png")
            .map_err(|e| format!("Failed to create secure temp file: {e}"))?;

        let mut img = ImageBuffer::<Rgb<u8>, Vec<u8>>::new(CANVAS_WIDTH as u32, CANVAS_HEIGHT as u32);
        for y in 0..CANVAS_HEIGHT {
            for x in 0..CANVAS_WIDTH {
                let [r, g, b] = self.get_pixel(x, y);
                img.put_pixel(x as u32, y as u32, Rgb([r, g, b]));
            }
        }
        img.write_to(&mut file, image::ImageFormat::Png)
            .map_err(|e| format!("Failed to write PNG: {e}"))?;
        Ok(temp_path)
    }

    // --- High-Level Widget Renderers ---

    /// Render Battery Status Screen
    /// Layout:
    /// - Y=1 to Y=6: Battery Frame with proportional fill bar (0-100%)
    /// - Y=9 to Y=13: Centered Percentage or 'LOW' in 3x5 font
    /// - Color: Red <= 20%, Yellow 21-40%, Green > 40%, Cyan when charging
    pub fn render_battery(percent: u8, charging: bool, is_low: bool) -> Self {
        let mut canvas = Canvas::new();
        let clamped = percent.min(100);

        // Color coding
        let bar_color = if charging {
            [0, 229, 255] // Cyan
        } else if clamped <= 20 || is_low {
            [239, 68, 68] // Bright Red
        } else if clamped <= 40 {
            [245, 158, 11] // Amber
        } else {
            [34, 197, 94] // Vibrant Green
        };

        let frame_color = if clamped <= 20 || is_low {
            [220, 38, 38] // Red outline for low battery alert
        } else {
            [120, 120, 120] // Grey
        };

        // Battery outer shell: X=1..13, Y=1..6 (13x6)
        canvas.draw_rect(1, 1, 13, 6, frame_color);

        // Terminal nib on right: X=14, Y=2..5 (1x4)
        for y in 2..=4 {
            canvas.set_pixel(14, y, frame_color);
        }

        // Inner fill area: X=2..12 (11 pixels wide), Y=2..5 (4 pixels high)
        let fill_width = if clamped == 0 {
            0
        } else {
            // Map 1..100 to 1..11
            ((clamped as usize * 11) + 50) / 100
        };

        if fill_width > 0 {
            canvas.fill_rect(2, 2, fill_width, 4, bar_color);
        }

        // If charging, draw a bright yellow lightning bolt over the battery
        if charging {
            let bolt = [255, 235, 59];
            canvas.set_pixel(7, 2, bolt);
            canvas.set_pixel(6, 3, bolt);
            canvas.set_pixel(7, 3, bolt);
            canvas.set_pixel(8, 3, bolt);
            canvas.set_pixel(7, 4, bolt);
            canvas.set_pixel(6, 5, bolt);
        }

        // Bottom text: Percentage or 'LOW'
        let text_color = if clamped <= 20 || is_low {
            [239, 68, 68]
        } else if charging {
            [0, 229, 255]
        } else {
            [240, 240, 240]
        };

        let label = if is_low && clamped <= 10 {
            "LOW".to_string()
        } else {
            format!("{clamped}%")
        };

        canvas.draw_text_centered(9, &label, text_color);

        canvas
    }

    /// Render Gauge / Progress Bar Widget
    /// - Y=1 to Y=5: Centered label (e.g. 'CPU', '75%')
    /// - Y=8 to Y=12: Progress gauge bar with background and fill
    pub fn render_gauge(label: &str, value: u32, max: u32, color: [u8; 3]) -> Self {
        let mut canvas = Canvas::new();

        // Label on top
        canvas.draw_text_centered(2, label, [240, 240, 240]);

        // Frame: X=1..15, Y=9..13
        canvas.draw_rect(1, 9, 14, 5, [80, 80, 80]);

        let fill_max = 12;
        let fill_w = if max == 0 {
            0
        } else {
            ((value as usize * fill_max) / (max as usize)).min(fill_max)
        };

        if fill_w > 0 {
            canvas.fill_rect(2, 10, fill_w, 3, color);
        }

        canvas
    }

    /// Render 2-line Card (e.g. 'DONE' / 'TEST' or 'CLAU' / 'DE')
    pub fn render_card(
        top: &str,
        bottom: &str,
        fg_top: [u8; 3],
        fg_bottom: [u8; 3],
        bg: [u8; 3],
    ) -> Self {
        let mut canvas = Canvas::with_bg(bg);
        canvas.draw_text_centered(2, top, fg_top);
        canvas.draw_text_centered(9, bottom, fg_bottom);
        canvas
    }

    /// Render Metric with small header and value
    pub fn render_metric(label: &str, value: &str, color: [u8; 3]) -> Self {
        let mut canvas = Canvas::new();
        canvas.draw_text_centered(2, label, [160, 160, 160]);
        canvas.draw_text_centered(9, value, color);
        canvas
    }

    /// Render Dual Sysmon (CPU + RAM) on a single 16x16 canvas
    /// - Y=0..6: CPU indicator with label "C", percent, and dynamic horizontal bar
    /// - Y=8..14: RAM indicator with label "R", percent, and dynamic horizontal bar
    pub fn render_sysmon(cpu: u32, ram: u32) -> Self {
        let mut canvas = Canvas::new();
        let cpu_clamped = cpu.min(100);
        let ram_clamped = ram.min(100);

        // CPU color coding
        let cpu_color = if cpu_clamped >= 85 {
            [239, 68, 68] // Bright Red
        } else if cpu_clamped >= 70 {
            [245, 158, 11] // Amber
        } else {
            [34, 197, 94] // Green
        };

        // RAM color coding
        let ram_color = if ram_clamped >= 85 {
            [239, 68, 68] // Bright Red
        } else if ram_clamped >= 75 {
            [236, 72, 153] // Pink / Magenta
        } else {
            [0, 229, 255] // Cyan
        };

        // 1. Top half: CPU (Y=1..6)
        // "C" header icon
        canvas.draw_text_3x5(1, 1, "C", [220, 220, 220]);
        // Percentage text
        let cpu_text = format!("{cpu_clamped}%");
        let cpu_w = Self::measure_text_3x5(&cpu_text);
        let cpu_x = 16_usize.saturating_sub(cpu_w);
        canvas.draw_text_3x5(cpu_x, 1, &cpu_text, cpu_color);

        // CPU bar at Y=6 (14 pixels wide: X=1..14)
        canvas.fill_rect(1, 6, 14, 1, [40, 40, 40]);
        let cpu_fill = ((cpu_clamped as usize * 14) + 50) / 100;
        if cpu_fill > 0 {
            canvas.fill_rect(1, 6, cpu_fill.min(14), 1, cpu_color);
        }

        // 2. Bottom half: RAM (Y=8..13)
        // "R" header icon
        canvas.draw_text_3x5(1, 8, "R", [220, 220, 220]);
        // Percentage text
        let ram_text = format!("{ram_clamped}%");
        let ram_w = Self::measure_text_3x5(&ram_text);
        let ram_x = 16_usize.saturating_sub(ram_w);
        canvas.draw_text_3x5(ram_x, 8, &ram_text, ram_color);

        // RAM bar at Y=13 (14 pixels wide: X=1..14)
        canvas.fill_rect(1, 13, 14, 1, [40, 40, 40]);
        let ram_fill = ((ram_clamped as usize * 14) + 50) / 100;
        if ram_fill > 0 {
            canvas.fill_rect(1, 13, ram_fill.min(14), 1, ram_color);
        }

        canvas
    }

    /// Parse 256 RGB colors (hex format) or raw matrix into Canvas
    pub fn from_hex_pixels(hex_input: &str) -> Result<Self, String> {
        let mut canvas = Canvas::new();
        let tokens: Vec<&str> = hex_input
            .split(|c: char| c.is_whitespace() || c == ',' || c == ';')
            .filter(|s| !s.is_empty())
            .collect();

        if tokens.len() == 256 {
            // 256 hex colors
            for (i, tok) in tokens.iter().enumerate() {
                let clean = tok.strip_prefix('#').unwrap_or(tok);
                if clean.len() == 6 {
                    let r = u8::from_str_radix(&clean[0..2], 16).map_err(|e| e.to_string())?;
                    let g = u8::from_str_radix(&clean[2..4], 16).map_err(|e| e.to_string())?;
                    let b = u8::from_str_radix(&clean[4..6], 16).map_err(|e| e.to_string())?;
                    canvas.pixels[i] = [r, g, b];
                }
            }
            return Ok(canvas);
        }

        if tokens.len() == 16 {
            // 16 rows of binary/hex
            for (y, tok) in tokens.iter().enumerate() {
                let clean = tok.strip_prefix('#').unwrap_or(tok);
                if clean.len() == 4 {
                    // 16-bit hex row (4 hex digits = 16 bits)
                    let row_bits = u16::from_str_radix(clean, 16).map_err(|e| e.to_string())?;
                    for x in 0..16 {
                        if ((row_bits >> (15 - x)) & 1) == 1 {
                            canvas.set_pixel(x, y, [255, 255, 255]);
                        }
                    }
                }
            }
            return Ok(canvas);
        }

        Err(format!(
            "Expected 256 hex colors (e.g. #FF0000 ...) or 16 rows of 4-digit hex, got {} tokens",
            tokens.len()
        ))
    }
}

/// 3x5 compact bitmap font definition.
/// Returns (width, [row0, row1, row2, row3, row4]).
fn glyph_3x5(c: char) -> (usize, [u8; 5]) {
    match c.to_ascii_uppercase() {
        '0' => (3, [0b111, 0b101, 0b101, 0b101, 0b111]),
        '1' => (1, [0b1, 0b1, 0b1, 0b1, 0b1]),
        '2' => (3, [0b111, 0b001, 0b111, 0b100, 0b111]),
        '3' => (3, [0b111, 0b001, 0b111, 0b001, 0b111]),
        '4' => (3, [0b101, 0b101, 0b111, 0b001, 0b001]),
        '5' => (3, [0b111, 0b100, 0b111, 0b001, 0b111]),
        '6' => (3, [0b111, 0b100, 0b111, 0b101, 0b111]),
        '7' => (3, [0b111, 0b001, 0b010, 0b010, 0b010]),
        '8' => (3, [0b111, 0b101, 0b111, 0b101, 0b111]),
        '9' => (3, [0b111, 0b101, 0b111, 0b001, 0b111]),
        '%' => (3, [0b101, 0b001, 0b010, 0b100, 0b101]),
        'A' => (3, [0b010, 0b101, 0b111, 0b101, 0b101]),
        'B' => (3, [0b110, 0b101, 0b110, 0b101, 0b110]),
        'C' => (3, [0b111, 0b100, 0b100, 0b100, 0b111]),
        'D' => (3, [0b110, 0b101, 0b101, 0b101, 0b110]),
        'E' => (3, [0b111, 0b100, 0b110, 0b100, 0b111]),
        'F' => (3, [0b111, 0b100, 0b110, 0b100, 0b100]),
        'G' => (3, [0b111, 0b100, 0b101, 0b101, 0b111]),
        'H' => (3, [0b101, 0b101, 0b111, 0b101, 0b101]),
        'I' => (3, [0b111, 0b010, 0b010, 0b010, 0b111]),
        'J' => (3, [0b001, 0b001, 0b001, 0b101, 0b010]),
        'K' => (3, [0b101, 0b110, 0b100, 0b110, 0b101]),
        'L' => (3, [0b100, 0b100, 0b100, 0b100, 0b111]),
        'M' => (5, [0b10001, 0b11011, 0b10101, 0b10001, 0b10001]),
        'N' => (3, [0b101, 0b111, 0b111, 0b101, 0b101]),
        'O' => (3, [0b111, 0b101, 0b101, 0b101, 0b111]),
        'P' => (3, [0b111, 0b101, 0b111, 0b100, 0b100]),
        'Q' => (3, [0b111, 0b101, 0b101, 0b111, 0b001]),
        'R' => (3, [0b110, 0b101, 0b110, 0b101, 0b101]),
        'S' => (3, [0b011, 0b100, 0b010, 0b001, 0b110]),
        'T' => (3, [0b111, 0b010, 0b010, 0b010, 0b010]),
        'U' => (3, [0b101, 0b101, 0b101, 0b101, 0b111]),
        'V' => (3, [0b101, 0b101, 0b101, 0b101, 0b010]),
        'W' => (5, [0b10001, 0b10001, 0b10101, 0b11011, 0b10001]),
        'X' => (3, [0b101, 0b101, 0b010, 0b101, 0b101]),
        'Y' => (3, [0b101, 0b101, 0b010, 0b010, 0b010]),
        'Z' => (3, [0b111, 0b001, 0b010, 0b100, 0b111]),
        ' ' => (2, [0, 0, 0, 0, 0]),
        '!' => (1, [0b1, 0b1, 0b1, 0b0, 0b1]),
        ':' => (1, [0b0, 0b1, 0b0, 0b1, 0b0]),
        '.' => (1, [0b0, 0b0, 0b0, 0b0, 0b1]),
        '-' => (3, [0b000, 0b000, 0b111, 0b000, 0b000]),
        '+' => (3, [0b010, 0b111, 0b010, 0b000, 0b000]),
        '/' => (3, [0b001, 0b001, 0b010, 0b100, 0b100]),
        '?' => (3, [0b110, 0b001, 0b010, 0b000, 0b010]),
        _ => (3, [0b111, 0b101, 0b101, 0b101, 0b111]), // fallback box
    }
}
