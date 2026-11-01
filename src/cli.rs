use crate::battery;
use crate::canvas::{Canvas, CanvasType};
use crate::config::Config;
use crate::daemon::{self, Command as DaemonCommand, Priority};
use crate::device;
use crate::icons;
use crate::integrations;
use crate::sysmon;

use clap::{CommandFactory, Parser, Subcommand, ValueHint};
use clap_complete::{generate, Shell};
use env_logger::{Builder, Env};
use std::error::Error;
use std::path::PathBuf;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;

#[derive(Parser, Debug)]
#[command(name = "ditoo", author, version, about = "Divoom Ditoo Pro Arch Linux Notification & Display Controller")]
struct Cli {
    #[arg(short, long, global = true)]
    verbose: bool,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Connect to paired Ditoo Pro device via Bluetooth and restore clock
    Connect,

    /// Show current device status and information
    Status,

    /// Switch Ditoo Pro back to native clock mode
    Clock,

    /// Synchronize Ditoo Pro hardware RTC clock with Linux system time
    SyncTime,

    /// Set screen brightness level (0-100)
    Brightness {
        #[arg(value_parser = clap::value_parser!(u8).range(0..=100))]
        level: u8,
    },

    /// Control screen power (on / off)
    Screen {
        /// State: on, off, 1, 0, true, false
        #[arg(default_value = "on")]
        state: String,
    },

    /// Display or update Divoom Ditoo Pro device battery status and level
    Battery {
        /// Set/update device battery percentage (0-100). If omitted, displays current device battery state
        #[arg(value_parser = clap::value_parser!(u8).range(0..=100))]
        percent: Option<u8>,

        /// Mark Ditoo Pro device as plugged in / charging
        #[arg(short, long)]
        charging: bool,

        /// Mark Ditoo Pro device as running on battery (not charging)
        #[arg(long)]
        not_charging: bool,

        /// Mark as low battery warning
        #[arg(short, long)]
        low: bool,

        /// Display duration in seconds (default: 5.0)
        #[arg(short, long)]
        duration: Option<f64>,

        /// Only print battery status without changing the Ditoo screen
        #[arg(long)]
        status_only: bool,
    },

    /// Display live Linux system resource metrics (CPU & RAM) on Ditoo Pro
    Sysmon {
        /// Display duration in seconds (default: 5.0)
        #[arg(short, long)]
        duration: Option<f64>,

        /// Only print metrics to terminal without displaying on Ditoo
        #[arg(long)]
        status_only: bool,
    },

    /// Display generalized widgets, gauges, cards, or raw pixels on the screen
    #[command(subcommand)]
    Show(ShowCommands),

    /// Display a desktop/task notification and automatically return to the clock
    Notify {
        /// Notification message
        message: String,

        /// Optional notification title
        #[arg(short, long)]
        title: Option<String>,

        /// Icon name (done, error, alert, claude, codex, antigravity, busy, disk, mail, etc.) or path to an image file
        #[arg(short, long, value_hint = ValueHint::FilePath)]
        icon: Option<String>,

        /// Display duration in seconds (default: 5.0)
        #[arg(short, long)]
        duration: Option<f64>,

        /// Notification priority (low, normal, high, urgent)
        #[arg(short, long, default_value = "normal")]
        priority: Priority,

        /// Play an audio notification sound
        #[arg(short, long)]
        sound: bool,
    },

    /// Display static text directly
    Text {
        /// Text string to display
        text: String,

        /// Text color (e.g. green, red, cyan, #FF0000)
        #[arg(short, long, default_value = "white")]
        color: String,
    },

    /// Display an image file directly
    Image {
        /// Path to PNG, JPEG, GIF, BMP, or WebP image
        #[arg(value_hint = ValueHint::FilePath)]
        path: PathBuf,
    },

    /// Display an animation file directly (.divoom16)
    Anim {
        /// Path to .divoom16 file
        #[arg(value_hint = ValueHint::FilePath)]
        path: PathBuf,
    },

    /// Start the background notification daemon
    Daemon,

    /// Start desktop notification monitor (forwarding from Dunst/DBus)
    MonitorDesktop,

    /// Configure integrations with AI coding tools (Claude Code, Codex)
    Integrate {
        /// Target tool: claude, codex, or all
        #[arg(default_value = "all")]
        target: String,
    },

    /// Showcase and cycle through all built-in pixel art icons and widgets on the screen
    Showcase {
        /// Display duration per icon in seconds (default: 5.0)
        #[arg(short, long, default_value_t = 5.0)]
        interval: f64,
    },

    /// Generate shell completion scripts (bash, zsh, fish, elvish, powershell)
    Completions {
        /// Shell to generate completions for: bash, zsh, fish, elvish, powershell
        #[arg(value_enum)]
        shell: Shell,

        /// Automatically install completion scripts into user shell completion directory
        #[arg(short, long)]
        install: bool,
    },
}

#[derive(Subcommand, Debug)]
enum ShowCommands {
    /// Show battery widget (reads system or uses percentage)
    Battery {
        #[arg(value_parser = clap::value_parser!(u8).range(0..=100))]
        percent: Option<u8>,
        #[arg(short, long)]
        charging: bool,
        #[arg(short, long)]
        low: bool,
        #[arg(short, long)]
        duration: Option<f64>,
    },

    /// Show a progress gauge bar with label
    Gauge {
        /// Label to display (e.g. CPU, RAM, DL)
        label: String,

        /// Current progress value
        value: u32,

        /// Maximum progress value (default: 100)
        #[arg(default_value = "100")]
        max: u32,

        /// Bar color (green, cyan, red, #00FF00)
        #[arg(short, long, default_value = "green")]
        color: String,

        /// Display duration in seconds (default: 5.0)
        #[arg(short, long)]
        duration: Option<f64>,
    },

    /// Show a 2-line high-contrast card
    Card {
        /// Top line text
        top: String,

        /// Bottom line text
        bottom: String,

        /// Top text color
        #[arg(long, default_value = "white")]
        top_color: String,

        /// Bottom text color
        #[arg(long, default_value = "green")]
        bottom_color: String,

        /// Background color
        #[arg(long, default_value = "black")]
        bg: String,

        /// Display duration in seconds (default: 5.0)
        #[arg(short, long)]
        duration: Option<f64>,
    },

    /// Show a single metric with label and value
    Metric {
        /// Metric label (e.g. TEMP, LOAD, PULL)
        label: String,

        /// Metric value (e.g. 45C, 1.2, #42)
        value: String,

        /// Value color
        #[arg(short, long, default_value = "cyan")]
        color: String,

        /// Display duration in seconds (default: 5.0)
        #[arg(short, long)]
        duration: Option<f64>,
    },

    /// Display raw 16x16 pixel matrix from 256 hex colors or 16 rows of hex
    Pixels {
        /// 256 space/comma separated hex colors (#RRGGBB) or 16 4-digit hex rows
        data: String,

        /// Display duration in seconds (default: 5.0)
        #[arg(short, long)]
        duration: Option<f64>,
    },

    /// Show dual CPU & RAM live system monitor
    Sysmon {
        /// Override CPU percentage (0-100)
        #[arg(long)]
        cpu: Option<u32>,

        /// Override RAM percentage (0-100)
        #[arg(long)]
        ram: Option<u32>,

        /// Display duration in seconds (default: 5.0)
        #[arg(short, long)]
        duration: Option<f64>,
    },

    /// Show single CPU usage gauge
    Cpu {
        /// Override CPU percentage (0-100)
        #[arg(value_parser = clap::value_parser!(u32).range(0..=100))]
        cpu: Option<u32>,

        /// Display duration in seconds (default: 5.0)
        #[arg(short, long)]
        duration: Option<f64>,
    },

    /// Show single RAM usage gauge
    Ram {
        /// Override RAM percentage (0-100)
        #[arg(value_parser = clap::value_parser!(u32).range(0..=100))]
        ram: Option<u32>,

        /// Display duration in seconds (default: 5.0)
        #[arg(short, long)]
        duration: Option<f64>,
    },

    /// Show single Disk filesystem usage gauge
    Disk {
        /// Override Disk percentage (0-100)
        #[arg(value_parser = clap::value_parser!(u32).range(0..=100))]
        disk: Option<u32>,

        /// Display duration in seconds (default: 5.0)
        #[arg(short, long)]
        duration: Option<f64>,
    },
}

async fn display_canvas(canvas_type: CanvasType, duration: Option<f64>) -> Result<(), Box<dyn Error>> {
    // 1. Try sending to daemon first via Unix socket
    if let Ok(mut stream) = UnixStream::connect(daemon::socket_path()).await {
        let cmd = DaemonCommand::Display {
            canvas: canvas_type,
            duration,
            priority: Priority::Normal,
        };
        let json = serde_json::to_string(&cmd)?;
        stream.write_all(format!("{json}\n").as_bytes()).await?;
        return Ok(());
    }

    // 2. Direct fallback execution
    let canvas = canvas_type.render().map_err(|e| format!("Render error: {e}"))?;
    let temp_path = canvas.to_temp_png()?;
    let config = Config::load();
    let (dev, _) = device::spawn_device_worker(config.device_mac).await?;
    let _ = dev.show_image(temp_path.clone()).await;
    let _ = std::fs::remove_file(temp_path);

    let dur = duration.unwrap_or(config.default_duration);
    tokio::time::sleep(Duration::from_secs_f64(dur.max(1.0))).await;
    let _ = dev.show_clock().await;
    Ok(())
}

pub async fn run_cli() -> Result<(), Box<dyn Error>> {
    let cli = Cli::parse();

    let log_level = if cli.verbose { "debug" } else { "info" };
    Builder::from_env(Env::default().default_filter_or(log_level)).init();

    match cli.command {
        Commands::Connect => {
            let config = Config::load();
            let mac = match config.device_mac {
                Some(m) => m,
                None => {
                    let addr = device::resolve_device(None).await?;
                    addr.to_string()
                }
            };

            println!("Connecting to paired Ditoo Pro ({mac})...");
            let output = tokio::process::Command::new("bluetoothctl")
                .args(["connect", &mac])
                .output()
                .await?;

            if !output.status.success() {
                let err = String::from_utf8_lossy(&output.stderr);
                return Err(format!("Failed to connect via bluetoothctl: {err}").into());
            }

            tokio::time::sleep(Duration::from_secs(2)).await;

            let (dev, _) = device::spawn_device_worker(Some(mac)).await?;
            let _ = dev.sync_time().await;
            let _ = dev.show_clock().await;
            println!("Connected successfully! RTC time synchronized and clock mode restored.");
        }

        Commands::Status => {
            if let Ok(mut stream) = UnixStream::connect(daemon::socket_path()).await {
                let cmd = DaemonCommand::Status;
                let json = serde_json::to_string(&cmd)?;
                stream.write_all(format!("{json}\n").as_bytes()).await?;

                let mut reader = BufReader::new(stream).lines();
                if let Some(line) = reader.next_line().await? {
                    println!("{line}");
                    return Ok(());
                }
            }

            // Fallback: direct connection
            let config = Config::load();
            let (dev, _) = device::spawn_device_worker(config.device_mac).await?;
            let status = dev.get_status().await?;
            println!("{}", serde_json::to_string_pretty(&status)?);
        }

        Commands::Clock => {
            if let Ok(mut stream) = UnixStream::connect(daemon::socket_path()).await {
                let cmd = DaemonCommand::Clock;
                let json = serde_json::to_string(&cmd)?;
                stream.write_all(format!("{json}\n").as_bytes()).await?;
                println!("Restored clock mode via daemon.");
                return Ok(());
            }

            // Direct connection
            let config = Config::load();
            let (dev, _) = device::spawn_device_worker(config.device_mac).await?;
            dev.show_clock().await?;
            println!("Restored Ditoo Pro native clock mode.");
        }

        Commands::SyncTime => {
            if let Ok(mut stream) = UnixStream::connect(daemon::socket_path()).await {
                let cmd = DaemonCommand::SyncTime;
                let json = serde_json::to_string(&cmd)?;
                stream.write_all(format!("{json}\n").as_bytes()).await?;
                println!("Synchronized clock via daemon.");
                return Ok(());
            }

            // Direct connection
            let config = Config::load();
            let (dev, _) = device::spawn_device_worker(config.device_mac).await?;
            dev.sync_time().await?;
            println!("Synchronized Ditoo Pro RTC with local system time.");
        }

        Commands::Brightness { level } => {
            if let Ok(mut stream) = UnixStream::connect(daemon::socket_path()).await {
                let cmd = DaemonCommand::Brightness { level };
                let json = serde_json::to_string(&cmd)?;
                stream.write_all(format!("{json}\n").as_bytes()).await?;
                println!("Set brightness to {level} via daemon.");
                return Ok(());
            }

            // Direct connection
            let config = Config::load();
            let (dev, _) = device::spawn_device_worker(config.device_mac).await?;
            dev.set_brightness(level).await?;
            println!("Set screen brightness to {level}.");
        }

        Commands::Screen { state } => {
            let on = match state.to_lowercase().as_str() {
                "on" | "1" | "true" | "yes" => true,
                "off" | "0" | "false" | "no" => false,
                _ => {
                    eprintln!("Invalid state '{state}'. Use 'on' or 'off'.");
                    return Ok(());
                }
            };

            if let Ok(mut stream) = UnixStream::connect(daemon::socket_path()).await {
                let cmd = DaemonCommand::Screen { on };
                let json = serde_json::to_string(&cmd)?;
                stream.write_all(format!("{json}\n").as_bytes()).await?;
                println!("Screen power set to {} via daemon.", if on { "ON" } else { "OFF" });
                return Ok(());
            }

            let config = Config::load();
            let (dev, _) = device::spawn_device_worker(config.device_mac).await?;
            dev.set_screen_power(on).await?;
            println!("Screen power set to {}.", if on { "ON" } else { "OFF" });
        }

        Commands::Battery {
            percent,
            charging,
            not_charging,
            low,
            duration,
            status_only,
        } => {
            let charging_opt = if charging {
                Some(true)
            } else if not_charging {
                Some(false)
            } else {
                None
            };

            // 1. Try sending via daemon socket
            if let Ok(mut stream) = UnixStream::connect(daemon::socket_path()).await {
                let cmd = DaemonCommand::Battery {
                    percent,
                    charging: charging_opt,
                    display: !status_only,
                    duration,
                };
                let json = serde_json::to_string(&cmd)?;
                stream.write_all(format!("{json}\n").as_bytes()).await?;

                let mut reader = BufReader::new(stream).lines();
                if let Some(line) = reader.next_line().await? {
                    if let Ok(val) = serde_json::from_str::<serde_json::Value>(&line)
                        && let Some(bat) = val.get("battery") {
                            let pct = bat.get("percent").and_then(|v| v.as_u64()).unwrap_or(0);
                            let chg = bat.get("is_charging").and_then(|v| v.as_bool()).unwrap_or(false);
                            let chg_str = if chg { " [Charging ⚡]" } else { "" };
                            println!("Ditoo Pro Battery: {pct}%{chg_str}");
                            if pct < 10 && !chg {
                                println!("⚠️  BATTERY CRITICAL (<10%): Battery alert screen is active until device is plugged in.");
                            } else if pct <= 20 && !chg {
                                println!("⚠️  Low battery warning (<=20%). Please recharge soon.");
                            }
                            return Ok(());
                        }
                    println!("{line}");
                }
                return Ok(());
            }

            // 2. Direct offline fallback
            let mut state = battery::DeviceBatteryState::load();
            if let Some(p) = percent {
                state.percent = p;
            }
            if let Some(c) = charging_opt {
                state.is_charging = c;
            }
            if percent.is_some() || charging_opt.is_some() {
                state.updated_at = chrono::Local::now();
                let _ = state.save();
            }

            println!("{}", state.description());
            if !status_only {
                display_canvas(
                    CanvasType::Battery {
                        percent: state.percent,
                        charging: state.is_charging,
                        is_low: low || state.is_low(),
                    },
                    duration,
                )
                .await?;
            }
        }

        Commands::Sysmon { duration, status_only } => {
            // 1. Try sending via daemon socket
            if let Ok(mut stream) = UnixStream::connect(daemon::socket_path()).await {
                let cmd = DaemonCommand::Sysmon {
                    display: !status_only,
                    duration,
                };
                let json = serde_json::to_string(&cmd)?;
                stream.write_all(format!("{json}\n").as_bytes()).await?;

                let mut reader = BufReader::new(stream).lines();
                if let Some(line) = reader.next_line().await? {
                    if let Ok(val) = serde_json::from_str::<serde_json::Value>(&line)
                        && let Some(s) = val.get("sysmon") {
                            let cpu = s.get("cpu_pct").and_then(|v| v.as_f64()).unwrap_or(0.0);
                            let ram = s.get("ram_pct").and_then(|v| v.as_f64()).unwrap_or(0.0);
                            let used = s.get("ram_used_mb").and_then(|v| v.as_u64()).unwrap_or(0);
                            let total = s.get("ram_total_mb").and_then(|v| v.as_u64()).unwrap_or(0);
                            let disk = s.get("disk_pct").and_then(|v| v.as_f64()).unwrap_or(0.0);
                            let disk_used = s.get("disk_used_gb").and_then(|v| v.as_f64()).unwrap_or(0.0);
                            let disk_total = s.get("disk_total_gb").and_then(|v| v.as_f64()).unwrap_or(0.0);
                            println!("Linux System Monitor:");
                            println!("  CPU Usage : {:.1}%", cpu);
                            println!("  RAM Usage : {:.1}% ({}/{} MB)", ram, used, total);
                            println!("  Disk Usage: {:.1}% ({:.1}/{:.1} GB)", disk, disk_used, disk_total);
                            if disk >= 85.0 {
                                println!("  ⚠️  WARNING: Disk usage is critical ({:.1}% >= 85%)!", disk);
                            }
                            return Ok(());
                        }
                    println!("{line}");
                }
                return Ok(());
            }

            // 2. Direct offline fallback
            let snap = sysmon::read_instant_snapshot();
            println!("Linux System Monitor:");
            println!("  CPU Usage : {:.1}%", snap.cpu_pct);
            println!("  RAM Usage : {:.1}% ({}/{} MB)", snap.ram_pct, snap.ram_used_mb, snap.ram_total_mb);
            println!("  Disk Usage: {:.1}% ({:.1}/{:.1} GB)", snap.disk_pct, snap.disk_used_gb, snap.disk_total_gb);
            if snap.disk_pct >= 85.0 {
                println!("  ⚠️  WARNING: Disk usage is critical ({:.1}% >= 85%)!", snap.disk_pct);
            }

            if !status_only {
                display_canvas(
                    CanvasType::Sysmon {
                        cpu: snap.cpu_pct as u32,
                        ram: snap.ram_pct as u32,
                    },
                    duration,
                )
                .await?;
            }
        }

        Commands::Show(cmd) => match cmd {
            ShowCommands::Battery {
                percent,
                charging,
                low,
                duration,
            } => {
                let state = battery::DeviceBatteryState::load();
                let pct = percent.unwrap_or(state.percent);
                let chg = charging || state.is_charging;
                let is_low = low || (pct <= 20 && !chg);

                println!("Displaying Ditoo Pro battery: {pct}% (charging: {chg}, low_warning: {is_low})...");
                display_canvas(
                    CanvasType::Battery {
                        percent: pct,
                        charging: chg,
                        is_low,
                    },
                    duration,
                )
                .await?;
            }
            ShowCommands::Sysmon { cpu, ram, duration } => {
                let (cpu_val, ram_val) = match (cpu, ram) {
                    (Some(c), Some(r)) => (c, r),
                    _ => {
                        let snap = sysmon::read_instant_snapshot();
                        (cpu.unwrap_or(snap.cpu_pct as u32), ram.unwrap_or(snap.ram_pct as u32))
                    }
                };

                println!("Displaying dual Sysmon: CPU {cpu_val}%, RAM {ram_val}%...");
                display_canvas(CanvasType::Sysmon { cpu: cpu_val, ram: ram_val }, duration).await?;
            }
            ShowCommands::Cpu { cpu, duration } => {
                let cpu_val = match cpu {
                    Some(c) => c,
                    None => {
                        let snap = sysmon::read_instant_snapshot();
                        snap.cpu_pct as u32
                    }
                };
                let color = if cpu_val >= 90 { "red" } else if cpu_val >= 75 { "amber" } else { "green" };
                println!("Displaying CPU gauge: {cpu_val}% [{color}]...");
                display_canvas(
                    CanvasType::Gauge {
                        label: "CPU".into(),
                        value: cpu_val,
                        max: 100,
                        color: color.into(),
                    },
                    duration,
                )
                .await?;
            }
            ShowCommands::Ram { ram, duration } => {
                let ram_val = match ram {
                    Some(r) => r,
                    None => {
                        let snap = sysmon::read_instant_snapshot();
                        snap.ram_pct as u32
                    }
                };
                let color = if ram_val >= 90 { "red" } else if ram_val >= 75 { "magenta" } else { "cyan" };
                println!("Displaying RAM gauge: {ram_val}% [{color}]...");
                display_canvas(
                    CanvasType::Gauge {
                        label: "RAM".into(),
                        value: ram_val,
                        max: 100,
                        color: color.into(),
                    },
                    duration,
                )
                .await?;
            }
            ShowCommands::Disk { disk, duration } => {
                let disk_val = match disk {
                    Some(d) => d,
                    None => {
                        let snap = sysmon::read_instant_snapshot();
                        snap.disk_pct as u32
                    }
                };
                let color = if disk_val >= 85 { "red" } else if disk_val >= 70 { "amber" } else { "cyan" };
                println!("Displaying DISK gauge: {disk_val}% [{color}]...");
                display_canvas(
                    CanvasType::Gauge {
                        label: "DISK".into(),
                        value: disk_val,
                        max: 100,
                        color: color.into(),
                    },
                    duration,
                )
                .await?;
            }
            ShowCommands::Gauge {
                label,
                value,
                max,
                color,
                duration,
            } => {
                println!("Displaying gauge: '{label}' ({value}/{max}) [{color}]...");
                display_canvas(
                    CanvasType::Gauge {
                        label,
                        value,
                        max,
                        color,
                    },
                    duration,
                )
                .await?;
            }
            ShowCommands::Card {
                top,
                bottom,
                top_color,
                bottom_color,
                bg,
                duration,
            } => {
                println!("Displaying card: '{top}' / '{bottom}'...");
                display_canvas(
                    CanvasType::Card {
                        top,
                        bottom,
                        top_color,
                        bottom_color,
                        bg_color: bg,
                    },
                    duration,
                )
                .await?;
            }
            ShowCommands::Metric {
                label,
                value,
                color,
                duration,
            } => {
                println!("Displaying metric: '{label}': {value} [{color}]...");
                display_canvas(
                    CanvasType::Metric {
                        label,
                        value,
                        color,
                    },
                    duration,
                )
                .await?;
            }
            ShowCommands::Pixels { data, duration } => {
                println!("Displaying custom 16x16 pixel matrix...");
                display_canvas(CanvasType::Pixels { hex_data: data }, duration).await?;
            }
        },

        Commands::Notify {
            message,
            title,
            icon,
            duration,
            priority,
            sound,
        } => {
            // Try sending to running daemon first
            if let Ok(mut stream) = UnixStream::connect(daemon::socket_path()).await {
                let cmd = DaemonCommand::Notify {
                    message,
                    title,
                    icon,
                    duration,
                    priority,
                    sound,
                };
                let json = serde_json::to_string(&cmd)?;
                stream.write_all(format!("{json}\n").as_bytes()).await?;

                let mut reader = BufReader::new(stream).lines();
                if let Some(line) = reader.next_line().await? {
                    log::debug!("Daemon response: {line}");
                }
                return Ok(());
            }

            // Direct execution fallback
            log::info!("Daemon not active; running in direct notification mode.");
            let config = Config::load();
            let (dev, _) = device::spawn_device_worker(config.device_mac).await?;

            if sound {
                let sound_cmd = config.sound.command.clone();
                tokio::spawn(async move {
                    let parts: Vec<&str> = sound_cmd.split_whitespace().collect();
                    if let Some((bin, args)) = parts.split_first() {
                        let _ = tokio::process::Command::new(bin).args(args).output().await;
                    }
                });
            }

            let mut displayed = false;
            if let Some(ref icon_name) = icon
                && let Some(resolved) = icons::resolve_icon(icon_name)
                    && resolved.display(&dev).await.is_ok() {
                        displayed = true;
                    }

            if !displayed {
                let color = match priority {
                    Priority::Urgent | Priority::High => "red",
                    Priority::Low => "cyan",
                    Priority::Normal => "green",
                };
                let text: String = message.chars().take(6).collect();
                dev.show_text(text, color.to_string()).await?;
            }

            let dur = duration.unwrap_or(config.default_duration);
            tokio::time::sleep(Duration::from_secs_f64(dur.max(1.0))).await;
            dev.show_clock().await?;
        }

        Commands::Text { text, color } => {
            let config = Config::load();
            let (dev, _) = device::spawn_device_worker(config.device_mac).await?;
            dev.show_text(text.clone(), color).await?;
            println!("Displayed text: '{text}'.");
        }

        Commands::Image { path } => {
            let config = Config::load();
            let (dev, _) = device::spawn_device_worker(config.device_mac).await?;
            dev.show_image(path.clone()).await?;
            println!("Displayed image: {}.", path.display());
        }

        Commands::Anim { path } => {
            let config = Config::load();
            let (dev, _) = device::spawn_device_worker(config.device_mac).await?;
            dev.show_animation(path.clone()).await?;
            println!("Displayed animation: {}.", path.display());
        }

        Commands::Daemon => {
            daemon::run_daemon().await?;
        }

        Commands::MonitorDesktop => {
            let config = Config::load();
            integrations::run_desktop_monitor(config.desktop_monitor.filter_apps).await?;
        }

        Commands::Integrate { target } => match target.to_lowercase().as_str() {
            "claude" => {
                let msg = integrations::integrate_claude()?;
                println!("{msg}");
            }
            "codex" => {
                let msg = integrations::integrate_codex()?;
                println!("{msg}");
            }
            "all" => {
                let c_msg = integrations::integrate_claude()?;
                println!("{c_msg}");
                let x_msg = integrations::integrate_codex()?;
                println!("{x_msg}");
            }
            _ => {
                eprintln!("Unknown target '{target}'. Use 'claude', 'codex', or 'all'.");
            }
        },

        Commands::Showcase { interval } => {
            let config = Config::load();
            let (dev, _) = device::spawn_device_worker(config.device_mac).await?;
            let items: &[(&str, &str)] = &[
                ("mail", "Email Envelope"),
                ("chat", "Chat Bubble"),
                ("calendar", "Desk Calendar"),
                ("music", "Music Notes"),
                ("terminal", "Terminal Shell"),
                ("git", "Git Commit Tree"),
                ("bell", "Alert Bell Chime"),
                ("claude", "Claude Code AI"),
                ("codex", "OpenAI Codex AI"),
                ("antigravity", "Antigravity DeepMind AI"),
                ("done", "Task Success Checkmark"),
                ("info", "Information Notice"),
                ("alert", "Amber Warning"),
                ("error", "Error / Failure"),
                ("busy", "Busy Loading Spinner"),
                ("disk", "Floppy Disk Storage"),
            ];

            let total = items.len() + 3;
            println!("✨ Starting Ditoo icon showcase (cycling {total} items, {interval:.1}s each)...");
            for (idx, (name, desc)) in items.iter().enumerate() {
                println!("[{}/{}] Showing icon: '{}' — {} ({:.1}s)", idx + 1, total, name, desc, interval);
                if let Some(resolved) = icons::resolve_icon(name)
                    && let Err(e) = resolved.display(&dev).await {
                        eprintln!("Failed to display {name}: {e}");
                    }
                tokio::time::sleep(Duration::from_secs_f64(interval.max(1.0))).await;
            }

            // Also showcase dynamic battery canvas
            println!("[{}/{}] Showing dynamic canvas: 'battery' (85% Charging)...", total - 2, total);
            let bat_canvas = Canvas::render_battery(85, true, false);
            if let Ok(temp) = bat_canvas.to_temp_png() {
                let _ = dev.show_image(temp.clone()).await;
                let _ = std::fs::remove_file(temp);
            }
            tokio::time::sleep(Duration::from_secs_f64(interval.max(1.0))).await;

            // Also showcase dynamic sysmon canvas
            println!("[{}/{}] Showing dynamic canvas: 'sysmon' (Live CPU & RAM)...", total - 1, total);
            let snap = sysmon::read_instant_snapshot();
            let sysmon_canvas = Canvas::render_sysmon(snap.cpu_pct as u32, snap.ram_pct as u32);
            if let Ok(temp) = sysmon_canvas.to_temp_png() {
                let _ = dev.show_image(temp.clone()).await;
                let _ = std::fs::remove_file(temp);
            }
            tokio::time::sleep(Duration::from_secs_f64(interval.max(1.0))).await;

            // Also showcase dynamic disk canvas
            println!("[{}/{}] Showing dynamic canvas: 'disk' (88% Warning)...", total, total);
            let disk_canvas = Canvas::render_gauge("DISK", 88, 100, [239, 68, 68]);
            if let Ok(temp) = disk_canvas.to_temp_png() {
                let _ = dev.show_image(temp.clone()).await;
                let _ = std::fs::remove_file(temp);
            }
            tokio::time::sleep(Duration::from_secs_f64(interval.max(1.0))).await;

            println!("✨ Showcase complete! Restoring native digital clock...");
            dev.show_clock().await?;
            println!("✅ Clock restored.");
        }

        Commands::Completions { shell, install } => {
            if install {
                install_completions(shell)?;
            } else {
                let mut cmd = Cli::command();
                generate(shell, &mut cmd, "ditoo", &mut std::io::stdout());
            }
        }
    }

    Ok(())
}

fn install_completions(shell: Shell) -> Result<(), Box<dyn Error>> {
    match shell {
        Shell::Bash => {
            let dir = crate::config::resolve_data_dir().join("bash-completion").join("completions");
            std::fs::create_dir_all(&dir)?;

            let ditoo_file = dir.join("ditoo");
            let mut f1 = std::fs::File::create(&ditoo_file)?;
            let mut cmd1 = Cli::command();
            generate(Shell::Bash, &mut cmd1, "ditoo", &mut f1);

            let controller_file = dir.join("divoom-ditoo-controller");
            let mut f2 = std::fs::File::create(&controller_file)?;
            let mut cmd2 = Cli::command();
            generate(Shell::Bash, &mut cmd2, "divoom-ditoo-controller", &mut f2);

            println!("✅ Bash completions installed to:\n  • {}\n  • {}", ditoo_file.display(), controller_file.display());
            println!("Completions will automatically load in all new Bash interactive sessions.");
            println!("To test immediately in your current shell, run:\n  source {}", ditoo_file.display());
        }
        Shell::Zsh => {
            let dir = crate::config::resolve_data_dir().join("zsh").join("site-functions");
            std::fs::create_dir_all(&dir)?;

            let file = dir.join("_ditoo");
            let mut f = std::fs::File::create(&file)?;
            let mut cmd = Cli::command();
            generate(Shell::Zsh, &mut cmd, "ditoo", &mut f);
            println!("✅ Zsh completions installed to: {}", file.display());
        }
        Shell::Fish => {
            let dir = crate::config::resolve_config_dir().join("fish").join("completions");
            std::fs::create_dir_all(&dir)?;

            let file = dir.join("ditoo.fish");
            let mut f = std::fs::File::create(&file)?;
            let mut cmd = Cli::command();
            generate(Shell::Fish, &mut cmd, "ditoo", &mut f);
            println!("✅ Fish completions installed to: {}", file.display());
        }
        _ => {
            let mut cmd = Cli::command();
            generate(shell, &mut cmd, "ditoo", &mut std::io::stdout());
        }
    }
    Ok(())
}
