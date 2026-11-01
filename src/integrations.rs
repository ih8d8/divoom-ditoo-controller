use std::error::Error;
use std::fs;
use tokio::io::AsyncBufReadExt;

pub fn integrate_claude() -> Result<String, Box<dyn Error>> {
    let home = dirs::home_dir().ok_or("Cannot determine home directory")?;
    let claude_dir = home.join(".claude");
    fs::create_dir_all(&claude_dir)?;
    let settings_path = claude_dir.join("settings.json");

    let mut settings: serde_json::Value = if settings_path.exists() {
        let content = fs::read_to_string(&settings_path)?;
        serde_json::from_str(&content).unwrap_or(serde_json::json!({}))
    } else {
        serde_json::json!({})
    };

    if !settings.is_object() {
        settings = serde_json::json!({});
    }

    let obj = settings
        .as_object_mut()
        .ok_or("Failed to parse settings as JSON object")?;
    let hooks = obj.entry("hooks").or_insert_with(|| serde_json::json!({}));

    // Add/update Notification hook (idle_prompt or permission_prompt)
    let notify_hook = serde_json::json!([
        {
            "matcher": "permission_prompt|idle_prompt",
            "hooks": [
                {
                    "type": "command",
                    "command": "ditoo notify 'Claude: Action Required' --icon alert --priority high --sound",
                    "timeout": 5
                }
            ]
        }
    ]);
    hooks["Notification"] = notify_hook;

    // Add/update Stop hook (task complete)
    let stop_hook = serde_json::json!([
        {
            "hooks": [
                {
                    "type": "command",
                    "command": "ditoo notify 'Claude: Task Complete' --icon claude --priority normal --sound",
                    "timeout": 5
                }
            ]
        }
    ]);
    hooks["Stop"] = stop_hook;

    fs::write(&settings_path, serde_json::to_string_pretty(&settings)?)?;

    Ok(format!(
        "Successfully configured Claude Code hooks in {}",
        settings_path.display()
    ))
}

pub fn integrate_codex() -> Result<String, Box<dyn Error>> {
    let home = dirs::home_dir().ok_or("Cannot determine home directory")?;
    let codex_dir = home.join(".codex");
    fs::create_dir_all(&codex_dir)?;
    let config_path = codex_dir.join("config.toml");

    let mut content = if config_path.exists() {
        fs::read_to_string(&config_path)?
    } else {
        String::new()
    };

    if !content.contains("ditoo notify") {
        content.push_str("\n# Divoom Ditoo Notification Integration\n");
        content.push_str("notify = [\"ditoo\", \"notify\", \"Codex: Task Complete\", \"--icon\", \"codex\", \"--sound\"]\n");
        fs::write(&config_path, content)?;
    }

    Ok(format!(
        "Successfully configured Codex CLI notification in {}",
        config_path.display()
    ))
}

/// Simple desktop notification forwarder listening on DBus
pub async fn run_desktop_monitor(filter_apps: Vec<String>) -> Result<(), Box<dyn Error>> {
    log::info!(
        "Starting Desktop Notification Monitor (filtering: {:?})...",
        filter_apps
    );

    // Run dbus-monitor to listen to notification calls
    let mut child = tokio::process::Command::new("dbus-monitor")
        .args([
            "--session",
            "type='method_call',interface='org.freedesktop.Notifications',member='Notify'",
        ])
        .stdout(std::process::Stdio::piped())
        .spawn()?;

    let stdout = child
        .stdout
        .take()
        .ok_or("Failed to capture dbus-monitor stdout")?;
    let mut reader = tokio::io::BufReader::new(stdout).lines();

    let mut current_app = String::new();
    let mut current_summary = String::new();
    let mut string_count = 0;

    while let Some(line) = reader.next_line().await? {
        let trimmed = line.trim();
        if trimmed.starts_with("method call") {
            string_count = 0;
            current_app.clear();
            current_summary.clear();
        } else if trimmed.starts_with("string \"") && trimmed.ends_with('"') {
            string_count += 1;
            let val = &trimmed[8..trimmed.len() - 1];
            if string_count == 1 {
                current_app = val.to_string();
            } else if string_count == 3 {
                current_summary = val.to_string();

                // Check filter
                let app_lower = current_app.to_lowercase();
                let summary_lower = current_summary.to_lowercase();

                let matches_filter = filter_apps.iter().any(|filter| {
                    let f = filter.to_lowercase();
                    app_lower.contains(&f) || summary_lower.contains(&f)
                });

                if matches_filter {
                    log::info!(
                        "Desktop notification forwarded: [{}] {}",
                        current_app,
                        current_summary
                    );
                    let icon = if app_lower.contains("claude") {
                        "claude"
                    } else if app_lower.contains("codex") {
                        "codex"
                    } else if summary_lower.contains("error") || summary_lower.contains("fail") {
                        "error"
                    } else {
                        "done"
                    };

                    let _ = tokio::process::Command::new("ditoo")
                        .args(["notify", &current_summary, "--icon", icon])
                        .output()
                        .await;
                }
            }
        }
    }

    Ok(())
}
