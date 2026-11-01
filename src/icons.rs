use crate::canvas::Canvas;
use std::path::{Path, PathBuf};

// --- Embedded 16x16 Pixel Art Assets ---
pub static ICON_DONE: &[u8] = include_bytes!("../assets/icons/done.png");
pub static ICON_INFO: &[u8] = include_bytes!("../assets/icons/info.png");
pub static ICON_ERROR: &[u8] = include_bytes!("../assets/icons/error.png");
pub static ICON_ALERT: &[u8] = include_bytes!("../assets/icons/alert.png");
pub static ICON_CLAUDE: &[u8] = include_bytes!("../assets/icons/claude.png");
pub static ICON_CODEX: &[u8] = include_bytes!("../assets/icons/codex.png");
pub static ICON_ANTIGRAVITY: &[u8] = include_bytes!("../assets/icons/antigravity.png");
pub static ICON_BUSY: &[u8] = include_bytes!("../assets/icons/busy.png");
pub static ICON_BUSY_DIVOOM: &[u8] = include_bytes!("../assets/icons/busy.divoom16");
pub static ICON_BUSY_GIF: &[u8] = include_bytes!("../assets/icons/busy.gif");

// Extended desktop & application icons
pub static ICON_MAIL: &[u8] = include_bytes!("../assets/icons/mail.png");
pub static ICON_CHAT: &[u8] = include_bytes!("../assets/icons/chat.png");
pub static ICON_CALENDAR: &[u8] = include_bytes!("../assets/icons/calendar.png");
pub static ICON_MUSIC: &[u8] = include_bytes!("../assets/icons/music.png");
pub static ICON_TERMINAL: &[u8] = include_bytes!("../assets/icons/terminal.png");
pub static ICON_GIT: &[u8] = include_bytes!("../assets/icons/git.png");
pub static ICON_BELL: &[u8] = include_bytes!("../assets/icons/bell.png");
pub static ICON_DISK: &[u8] = include_bytes!("../assets/icons/disk.png");

#[derive(Debug, Clone)]
pub enum IconType {
    ImageBytes(&'static [u8]),
    AnimationBytes(&'static [u8]),
}

/// Resolved icon ready for rendering on the Ditoo Pro
#[derive(Debug, Clone)]
pub enum ResolvedIcon {
    /// File on disk (user assets directory, relative path, or system path)
    FilePath(PathBuf),
    /// Built-in image bytes compiled directly into the binary
    EmbeddedImage(&'static [u8]),
    /// Built-in animation bytes (.divoom16)
    EmbeddedAnimation(&'static [u8]),
    /// Procedurally generated 16x16 badge card
    Procedural(PathBuf),
}

impl ResolvedIcon {
    /// Render and send the icon to the physical Ditoo Pro device
    pub async fn display(
        &self,
        dev: &crate::device::DeviceHandle,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        match self {
            ResolvedIcon::FilePath(path) => {
                if path.extension().and_then(|e| e.to_str()) == Some("divoom16") {
                    dev.show_animation(path.clone()).await?;
                } else {
                    dev.show_image(path.clone()).await?;
                }
            }
            ResolvedIcon::EmbeddedImage(bytes) => {
                let temp = save_temp_icon(bytes, "png")?;
                let res = dev.show_image(temp.clone()).await;
                let _ = std::fs::remove_file(temp);
                res?;
            }
            ResolvedIcon::EmbeddedAnimation(bytes) => {
                let temp = save_temp_icon(bytes, "divoom16")?;
                let res = dev.show_animation(temp.clone()).await;
                let _ = std::fs::remove_file(temp);
                res?;
            }
            ResolvedIcon::Procedural(temp_path) => {
                let res = dev.show_image(temp_path.clone()).await;
                let _ = std::fs::remove_file(temp_path);
                res?;
            }
        }
        Ok(())
    }
}

/// Look up a built-in icon by name or alias
pub fn get_embedded_icon(name: &str) -> Option<IconType> {
    match name.to_ascii_lowercase().as_str() {
        "done" | "ok" | "success" | "check" | "finished" | "complete" => {
            Some(IconType::ImageBytes(ICON_DONE))
        }
        "info" | "default" | "msg" | "note" | "notice" => Some(IconType::ImageBytes(ICON_INFO)),
        "error" | "fail" | "failed" | "cross" | "bug" => Some(IconType::ImageBytes(ICON_ERROR)),
        "alert" | "warn" | "warning" | "approval" | "prompt" | "question" => {
            Some(IconType::ImageBytes(ICON_ALERT))
        }
        "bell" | "chime" | "ding" | "remind" => Some(IconType::ImageBytes(ICON_BELL)),
        "claude" | "claude-code" => Some(IconType::ImageBytes(ICON_CLAUDE)),
        "codex" | "openai" => Some(IconType::ImageBytes(ICON_CODEX)),
        "antigravity" | "agy" | "deepmind" => Some(IconType::ImageBytes(ICON_ANTIGRAVITY)),
        "busy" | "running" | "loading" | "build" | "spinner" => {
            Some(IconType::ImageBytes(ICON_BUSY))
        }
        "mail" | "email" | "inbox" | "letter" | "thunderbird" | "geary" | "mutt" | "neomutt" => {
            Some(IconType::ImageBytes(ICON_MAIL))
        }
        "chat" | "message" | "slack" | "discord" | "telegram" | "signal" | "matrix" | "element" => {
            Some(IconType::ImageBytes(ICON_CHAT))
        }
        "calendar" | "event" | "date" | "meeting" | "schedule" => {
            Some(IconType::ImageBytes(ICON_CALENDAR))
        }
        "music" | "audio" | "song" | "spotify" | "media" | "track" => {
            Some(IconType::ImageBytes(ICON_MUSIC))
        }
        "terminal" | "term" | "shell" | "bash" | "zsh" | "fish" | "cmd" | "cli" => {
            Some(IconType::ImageBytes(ICON_TERMINAL))
        }
        "git" | "github" | "gitlab" | "forgejo" | "pr" | "commit" => {
            Some(IconType::ImageBytes(ICON_GIT))
        }
        "disk" | "disc" | "storage" | "drive" | "hdd" | "ssd" => {
            Some(IconType::ImageBytes(ICON_DISK))
        }
        _ => None,
    }
}

/// Multi-tier icon resolver:
/// 1. Direct file path
/// 2. User config asset directory (~/.config/ditoo/icons/)
/// 3. Embedded built-in pixel art catalog
/// 4. Procedural 16x16 badge card fallback
pub fn resolve_icon(name: &str) -> Option<ResolvedIcon> {
    let raw_path = Path::new(name);
    // 1. Direct file path
    if raw_path.exists() {
        return Some(ResolvedIcon::FilePath(raw_path.to_path_buf()));
    }

    let clean_name = name.trim().to_ascii_lowercase();

    // 2. Unified User Asset Directory (~/.config/ditoo/icons/)
    // Keeps all user configurations and custom assets in one single place,
    // easily version-controlled within user dotfiles.
    let search_dirs = [
        crate::config::resolve_config_dir()
            .join("ditoo")
            .join("icons"),
        crate::config::resolve_data_dir()
            .join("ditoo")
            .join("icons"),
    ];

    let extensions = ["png", "divoom16", "gif", "bmp", "jpg", "jpeg", "webp"];
    for dir in &search_dirs {
        for ext in &extensions {
            let candidate = dir.join(format!("{clean_name}.{ext}"));
            if candidate.exists() {
                return Some(ResolvedIcon::FilePath(candidate));
            }
        }
    }

    // 3. Embedded icon catalog
    if let Some(embedded) = get_embedded_icon(&clean_name) {
        return match embedded {
            IconType::ImageBytes(b) => Some(ResolvedIcon::EmbeddedImage(b)),
            IconType::AnimationBytes(b) => Some(ResolvedIcon::EmbeddedAnimation(b)),
        };
    }

    // 4. Procedural 16x16 Badge Card Fallback
    if !clean_name.is_empty()
        && clean_name.len() <= 8
        && let Ok(temp_path) = generate_procedural_icon(&clean_name)
    {
        return Some(ResolvedIcon::Procedural(temp_path));
    }

    None
}

/// Generate a procedural 16x16 badge card when no matching icon file exists
pub fn generate_procedural_icon(
    label: &str,
) -> Result<PathBuf, Box<dyn std::error::Error + Send + Sync>> {
    let mut canvas = Canvas::new();
    let text = label.to_uppercase();

    // Deterministic palette based on label name
    let hash = label
        .bytes()
        .fold(0u32, |acc, b| acc.wrapping_add(b as u32));
    let bg_palette: &[[u8; 3]] = &[
        [15, 23, 42], // Navy Slate
        [30, 27, 75], // Deep Indigo
        [2, 44, 34],  // Deep Emerald
        [69, 10, 10], // Deep Crimson
        [67, 20, 7],  // Deep Amber
        [59, 7, 100], // Deep Purple
        [8, 47, 73],  // Deep Teal
    ];
    let bg = bg_palette[(hash as usize) % bg_palette.len()];
    canvas.fill(bg);

    // Subtle 1px outer frame
    let border = [
        bg[0].saturating_add(50),
        bg[1].saturating_add(50),
        bg[2].saturating_add(50),
    ];
    canvas.draw_rect(0, 0, 16, 16, border);

    // Typography in 3x5 font
    let text_len = text.len();
    if text_len <= 3 {
        // Single centered line
        let w = Canvas::measure_text_3x5(&text);
        let x = (16usize.saturating_sub(w)) / 2;
        canvas.draw_text_3x5(x, 5, &text, [255, 255, 255]);
    } else if text_len <= 6 {
        // 2 stacked lines
        let mid = text_len.div_ceil(2);
        let line1 = &text[..mid];
        let line2 = &text[mid..];
        let w1 = Canvas::measure_text_3x5(line1);
        let w2 = Canvas::measure_text_3x5(line2);
        let x1 = (16usize.saturating_sub(w1)) / 2;
        let x2 = (16usize.saturating_sub(w2)) / 2;
        canvas.draw_text_3x5(x1, 2, line1, [240, 240, 240]);
        canvas.draw_text_3x5(x2, 9, line2, [34, 197, 94]);
    } else {
        // Truncate to first 4 characters
        let chunk: String = text.chars().take(4).collect();
        let w = Canvas::measure_text_3x5(&chunk);
        let x = (16usize.saturating_sub(w)) / 2;
        canvas.draw_text_3x5(x, 5, &chunk, [255, 255, 255]);
    }

    let temp = canvas.to_temp_png()?;
    Ok(temp)
}

/// Helper to save icon bytes to a secure temporary path for display functions that expect paths
pub fn save_temp_icon(bytes: &[u8], extension: &str) -> std::io::Result<PathBuf> {
    use std::io::Write;
    let (temp, mut file) = crate::canvas::create_secure_temp_file("ditoo_icon", extension)?;
    file.write_all(bytes)?;
    Ok(temp)
}
