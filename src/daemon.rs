use std::collections::VecDeque;
use std::error::Error;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use bluer::Address;
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::{Mutex, mpsc};

use crate::battery::{self, HostBattery};
use crate::canvas::{Canvas, CanvasType};
use crate::config::Config;
use crate::device::{self, DeviceHandle};
use crate::icons;
use crate::sysmon::{self, CpuSample, MemInfo};

#[derive(
    Debug,
    Default,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Serialize,
    Deserialize,
    clap::ValueEnum,
)]
#[serde(rename_all = "lowercase")]
pub enum Priority {
    Low = 0,
    #[default]
    Normal = 1,
    High = 2,
    Urgent = 3,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Command {
    Notify {
        message: String,
        #[serde(default)]
        title: Option<String>,
        #[serde(default)]
        icon: Option<String>,
        #[serde(default)]
        duration: Option<f64>,
        #[serde(default)]
        priority: Priority,
        #[serde(default)]
        sound: bool,
    },
    Display {
        canvas: CanvasType,
        #[serde(default)]
        duration: Option<f64>,
        #[serde(default)]
        priority: Priority,
    },
    Battery {
        #[serde(default)]
        percent: Option<u8>,
        #[serde(default)]
        charging: Option<bool>,
        #[serde(default)]
        display: bool,
        #[serde(default)]
        duration: Option<f64>,
    },
    Sysmon {
        #[serde(default)]
        display: bool,
        #[serde(default)]
        duration: Option<f64>,
    },
    Clock,
    SyncTime,
    Brightness {
        level: u8,
    },
    Screen {
        on: bool,
    },
    Status,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationItem {
    pub message: String,
    pub title: Option<String>,
    pub icon: Option<String>,
    #[serde(default)]
    pub canvas: Option<CanvasType>,
    pub duration: f64,
    pub priority: Priority,
    pub sound: bool,
    #[serde(skip, default = "Instant::now")]
    #[allow(dead_code)]
    pub created_at: Instant,
}

pub const MAX_QUEUE_SIZE: usize = 64;

pub struct DaemonState {
    pub config: Config,
    pub device: Option<DeviceHandle>,
    pub device_addr: Option<Address>,
    pub queue: VecDeque<NotificationItem>,
    pub last_notification: Option<(String, Instant)>,
    pub is_displaying: bool,
    pub laptop_battery: Option<HostBattery>,
    pub is_battery_override: bool,
}

impl DaemonState {
    /// Inserts a notification into the priority queue with capacity capping (DoS mitigation).
    /// Items are sorted descending by priority (higher priority first).
    pub fn enqueue(&mut self, item: NotificationItem) {
        if self.queue.len() >= MAX_QUEUE_SIZE {
            // Drop lowest priority item from the back to maintain hard queue bounds
            if let Some(pos) = self.queue.iter().rposition(|q| q.priority < item.priority) {
                self.queue.remove(pos);
            } else {
                self.queue.pop_back();
            }
        }

        let mut insert_idx = self.queue.len();
        for (idx, q_item) in self.queue.iter().enumerate() {
            if item.priority > q_item.priority {
                insert_idx = idx;
                break;
            }
        }
        self.queue.insert(insert_idx, item);
    }
}

pub fn socket_path() -> PathBuf {
    if let Ok(runtime_dir) = std::env::var("XDG_RUNTIME_DIR") {
        PathBuf::from(runtime_dir).join("ditoo.sock")
    } else {
        PathBuf::from(format!("/tmp/ditoo-{}.sock", unsafe { libc::getuid() }))
    }
}

pub async fn run_daemon() -> Result<(), Box<dyn Error>> {
    let mut config = Config::load();
    let sock_path = socket_path();
    if sock_path.exists() {
        let _ = fs::remove_file(&sock_path);
    }

    log::info!("Starting Ditoo Notification Daemon...");
    log::info!("Listening on Unix socket: {}", sock_path.display());

    let listener = UnixListener::bind(&sock_path)?;

    // Restrict socket file permissions strictly to current user (0600)
    if let Ok(meta) = fs::metadata(&sock_path) {
        let mut perms = meta.permissions();
        perms.set_mode(0o600);
        let _ = fs::set_permissions(&sock_path, perms);
    }

    // Spawn Bluetooth device worker
    let (device_handle, device_addr) =
        match device::spawn_device_worker(config.device_mac.clone()).await {
            Ok((handle, addr)) => {
                log::info!("Target Ditoo Pro device: {}", addr);
                if config.device_mac.is_none() {
                    config.device_mac = Some(addr.to_string());
                    let _ = config.save();
                }
                (Some(handle), Some(addr))
            }
            Err(e) => {
                log::warn!(
                    "Could not immediately connect to Ditoo Pro: {}. Will retry on demand.",
                    e
                );
                (None, None)
            }
        };

    let initial_battery = battery::get_host_battery();
    let initial_battery_override = initial_battery.as_ref().is_some_and(|b| {
        b.should_override_clock_with_threshold(config.laptop_battery.critical_threshold)
    });

    let state = Arc::new(Mutex::new(DaemonState {
        config: config.clone(),
        device: device_handle.clone(),
        device_addr,
        queue: VecDeque::new(),
        last_notification: None,
        is_displaying: false,
        laptop_battery: initial_battery.clone(),
        is_battery_override: initial_battery_override,
    }));

    let (tx_notify, mut rx_notify) = mpsc::channel::<()>(32);

    // Initial time sync & display setup
    if let Some(ref dev) = device_handle {
        log::info!("Initial RTC clock synchronization...");
        let _ = dev.sync_time().await;

        if initial_battery_override && let Some(ref bat) = initial_battery {
            log::warn!(
                "Laptop battery is critical (<10% and not charging). Displaying battery icon instead of clock."
            );
            let canvas = Canvas::render_battery(bat.percent, bat.is_charging, true);
            if let Ok(temp_path) = canvas.to_temp_png() {
                let _ = dev.show_image(temp_path.clone()).await;
                let _ = fs::remove_file(temp_path);
            }
        } else {
            log::info!("Setting native clock mode...");
            let _ = dev.show_clock().await;
        }
    }

    // Periodic time sync background task (hourly)
    let state_sync = Arc::clone(&state);
    let sync_mins = config.sync_interval_mins.max(10);
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(sync_mins * 60));
        interval.tick().await; // skip immediate first tick
        loop {
            interval.tick().await;
            let dev_opt = {
                let st = state_sync.lock().await;
                st.device.clone()
            };
            if let Some(dev) = dev_opt {
                log::info!("Periodic RTC clock synchronization...");
                let _ = dev.sync_time().await;
            }
        }
    });

    // Background monitor task: Ditoo device battery discharge/charge & Sysmon (CPU & RAM)
    let state_monitor = Arc::clone(&state);
    let tx_monitor = tx_notify.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(15));
        let mut last_cpu_sample = CpuSample::read().unwrap_or_default();
        let mut last_cpu_alert = Instant::now() - Duration::from_secs(3600);
        let mut last_ram_alert = Instant::now() - Duration::from_secs(3600);
        let mut last_disk_alert = Instant::now() - Duration::from_secs(3600);
        let mut last_low_battery_alert = Instant::now() - Duration::from_secs(3600);

        interval.tick().await; // skip immediate first tick
        loop {
            interval.tick().await;
            let now_instant = Instant::now();

            // 1. Host Laptop Battery Monitoring & State Transition
            let (dev_opt, should_restore_clock, should_override_battery, bat_info, low_warn_canvas) = {
                let mut st = state_monitor.lock().await;
                let current_battery = battery::get_host_battery();
                st.laptop_battery = current_battery.clone();

                let critical_threshold = st.config.laptop_battery.critical_threshold;
                let low_threshold = st.config.laptop_battery.low_threshold;
                let needs_override = current_battery
                    .as_ref()
                    .is_some_and(|b| b.should_override_clock_with_threshold(critical_threshold));

                let currently_overriding = st.is_battery_override;
                let mut restore_clk = false;
                let mut trigger_override = false;
                let mut low_canvas = None;

                if needs_override
                    && !currently_overriding
                    && !st.is_displaying
                    && st.queue.is_empty()
                {
                    // Transition: laptop battery dropped below critical threshold and not charging -> show battery icon
                    st.is_battery_override = true;
                    trigger_override = true;
                } else if !needs_override && currently_overriding {
                    // Transition: plugged into AC power or battery rose -> restore clock!
                    st.is_battery_override = false;
                    restore_clk = true;
                } else if let Some(ref bat) = current_battery
                    && bat.is_low_with_threshold(low_threshold)
                    && !needs_override
                {
                    // Low warning (10..=20% and discharging)
                    let cooldown_secs = st.config.laptop_battery.warn_cooldown_mins * 60;
                    if now_instant.duration_since(last_low_battery_alert)
                        >= Duration::from_secs(cooldown_secs)
                    {
                        last_low_battery_alert = now_instant;
                        low_canvas = Some(CanvasType::Battery {
                            percent: bat.percent,
                            charging: bat.is_charging,
                            is_low: true,
                        });
                    }
                }

                (
                    st.device.clone(),
                    restore_clk,
                    trigger_override,
                    current_battery,
                    low_canvas,
                )
            };

            if let Some(ref dev) = dev_opt {
                if should_override_battery && let Some(ref bat) = bat_info {
                    log::warn!(
                        "Background monitor: laptop battery critical (<10%). Displaying battery icon instead of clock."
                    );
                    let canvas = Canvas::render_battery(bat.percent, bat.is_charging, true);
                    if let Ok(temp_path) = canvas.to_temp_png() {
                        let _ = dev.show_image(temp_path.clone()).await;
                        let _ = fs::remove_file(temp_path);
                    }
                } else if should_restore_clock {
                    log::info!(
                        "Background monitor: laptop plugged in or recovered. Restoring clock."
                    );
                    let _ = dev.show_clock().await;
                }
            }

            if let Some(canvas) = low_warn_canvas {
                let pct = bat_info.as_ref().map_or(0, |b| b.percent);
                let mut st = state_monitor.lock().await;
                st.enqueue(NotificationItem {
                    message: format!("Laptop battery low: {}%", pct),
                    title: Some("Low Laptop Battery".into()),
                    icon: None,
                    canvas: Some(canvas),
                    duration: 8.0,
                    priority: Priority::High,
                    sound: false,
                    created_at: now_instant,
                });
                drop(st);
                let _ = tx_monitor.send(()).await;
            }

            // 2. Sysmon (CPU, RAM & Disk) Monitoring
            let (
                sysmon_enabled,
                cpu_thresh,
                ram_thresh,
                disk_thresh,
                disk_mount,
                cooldown_dur,
                alert_dur,
            ) = {
                let st = state_monitor.lock().await;
                (
                    st.config.sysmon.enabled,
                    st.config.sysmon.cpu_threshold,
                    st.config.sysmon.ram_threshold,
                    st.config.sysmon.disk_threshold,
                    st.config.sysmon.disk_mount.clone(),
                    Duration::from_secs(st.config.sysmon.cooldown_secs),
                    st.config.sysmon.alert_duration,
                )
            };

            if sysmon_enabled && let Some(curr_sample) = CpuSample::read() {
                let cpu_pct = curr_sample.usage_since(&last_cpu_sample);
                last_cpu_sample = curr_sample;

                let mem = MemInfo::read().unwrap_or_default();
                let ram_pct = mem.percent_used;

                let disk = sysmon::DiskInfo::read_mount(&disk_mount).unwrap_or_default();
                let disk_pct = disk.percent_used;

                let cpu_high = cpu_pct >= cpu_thresh;
                let ram_high = ram_pct >= ram_thresh;
                let disk_high = disk_pct >= disk_thresh;

                if cpu_high && now_instant.duration_since(last_cpu_alert) >= cooldown_dur {
                    last_cpu_alert = now_instant;
                    log::info!(
                        "Sysmon alert: High CPU usage ({:.1}% >= {:.1}%)",
                        cpu_pct,
                        cpu_thresh
                    );

                    let gauge_color = if cpu_pct >= 90.0 { "red" } else { "amber" };
                    let item = NotificationItem {
                        message: format!("CPU: {:.0}%", cpu_pct),
                        title: Some("High CPU".into()),
                        icon: None,
                        canvas: Some(CanvasType::Gauge {
                            label: "CPU".into(),
                            value: cpu_pct as u32,
                            max: 100,
                            color: gauge_color.into(),
                        }),
                        duration: alert_dur,
                        priority: Priority::High,
                        sound: false,
                        created_at: now_instant,
                    };

                    let mut st = state_monitor.lock().await;
                    st.enqueue(item);
                    drop(st);
                    let _ = tx_monitor.send(()).await;
                } else if ram_high && now_instant.duration_since(last_ram_alert) >= cooldown_dur {
                    last_ram_alert = now_instant;
                    log::info!(
                        "Sysmon alert: High RAM usage ({:.1}% >= {:.1}%)",
                        ram_pct,
                        ram_thresh
                    );

                    let item = NotificationItem {
                        message: format!("RAM: {:.0}%", ram_pct),
                        title: Some("High RAM".into()),
                        icon: None,
                        canvas: Some(CanvasType::Gauge {
                            label: "RAM".into(),
                            value: ram_pct as u32,
                            max: 100,
                            color: "magenta".into(),
                        }),
                        duration: alert_dur,
                        priority: Priority::High,
                        sound: false,
                        created_at: now_instant,
                    };

                    let mut st = state_monitor.lock().await;
                    st.enqueue(item);
                    drop(st);
                    let _ = tx_monitor.send(()).await;
                } else if disk_high && now_instant.duration_since(last_disk_alert) >= cooldown_dur {
                    last_disk_alert = now_instant;
                    log::warn!(
                        "Sysmon alert: High Disk usage ({:.1}% >= {:.1}%) on '{}'",
                        disk_pct,
                        disk_thresh,
                        disk_mount
                    );

                    let gauge_color = if disk_pct >= 90.0 { "red" } else { "amber" };
                    let item = NotificationItem {
                        message: format!("DISK: {:.0}%", disk_pct),
                        title: Some("High Disk".into()),
                        icon: Some("disk".into()),
                        canvas: Some(CanvasType::Gauge {
                            label: "DISK".into(),
                            value: disk_pct as u32,
                            max: 100,
                            color: gauge_color.into(),
                        }),
                        duration: alert_dur,
                        priority: Priority::High,
                        sound: false,
                        created_at: now_instant,
                    };

                    let mut st = state_monitor.lock().await;
                    st.enqueue(item);
                    drop(st);
                    let _ = tx_monitor.send(()).await;
                }
            }
        }
    });

    // Queue worker task
    let state_worker = Arc::clone(&state);
    tokio::spawn(async move {
        worker_loop(state_worker, &mut rx_notify).await;
    });

    // Accept IPC connections
    loop {
        match listener.accept().await {
            Ok((stream, _)) => {
                let st = Arc::clone(&state);
                let tx = tx_notify.clone();
                tokio::spawn(async move {
                    if let Err(e) = handle_client(stream, st, tx).await {
                        log::debug!("Client connection error: {}", e);
                    }
                });
            }
            Err(e) => {
                log::error!("Socket accept error: {}", e);
                tokio::time::sleep(Duration::from_millis(500)).await;
            }
        }
    }
}

async fn handle_client(
    stream: UnixStream,
    state: Arc<Mutex<DaemonState>>,
    tx_notify: mpsc::Sender<()>,
) -> Result<(), Box<dyn Error>> {
    let (reader, mut writer) = stream.into_split();
    let mut lines = BufReader::new(reader).lines();

    while let Some(line) = lines.next_line().await? {
        if line.trim().is_empty() {
            continue;
        }

        let cmd: Command = match serde_json::from_str(&line) {
            Ok(c) => c,
            Err(e) => {
                let resp =
                    serde_json::json!({"status": "error", "message": format!("Invalid JSON: {e}")});
                writer.write_all(format!("{resp}\n").as_bytes()).await?;
                continue;
            }
        };

        match cmd {
            Command::Notify {
                message,
                title,
                icon,
                duration,
                priority,
                sound,
            } => {
                let mut st = state.lock().await;
                let default_dur = st.config.default_duration;

                // Check duplicate suppression within 3s
                let now = Instant::now();
                if let Some((ref last_msg, ref last_time)) = st.last_notification
                    && last_msg == &message
                    && now.duration_since(*last_time) < Duration::from_secs(3)
                {
                    log::info!("Suppressed duplicate notification: {}", message);
                    let resp = serde_json::json!({"status": "ok", "suppressed": true});
                    writer.write_all(format!("{resp}\n").as_bytes()).await?;
                    continue;
                }
                st.last_notification = Some((message.clone(), now));

                // Check quiet hours
                let in_quiet = is_quiet_hours(&st.config);
                let should_sound = sound && !in_quiet;

                let item = NotificationItem {
                    message,
                    title,
                    icon,
                    canvas: None,
                    duration: duration.unwrap_or(default_dur),
                    priority,
                    sound: should_sound,
                    created_at: now,
                };

                st.enqueue(item);
                drop(st);

                let _ = tx_notify.send(()).await;
                let resp = serde_json::json!({"status": "ok", "queued": true});
                writer.write_all(format!("{resp}\n").as_bytes()).await?;
            }
            Command::Display {
                canvas,
                duration,
                priority,
            } => {
                let mut st = state.lock().await;
                let default_dur = st.config.default_duration;
                let now = Instant::now();

                let item = NotificationItem {
                    message: format!("Display: {:?}", canvas),
                    title: None,
                    icon: None,
                    canvas: Some(canvas),
                    duration: duration.unwrap_or(default_dur),
                    priority,
                    sound: false,
                    created_at: now,
                };

                st.enqueue(item);
                drop(st);

                let _ = tx_notify.send(()).await;
                let resp = serde_json::json!({"status": "ok", "queued": true});
                writer.write_all(format!("{resp}\n").as_bytes()).await?;
            }
            Command::Battery {
                percent,
                charging,
                display,
                duration,
            } => {
                let mut st = state.lock().await;
                let current_bat = battery::get_host_battery();
                let bat_state = if let Some(p) = percent {
                    let chg = charging.unwrap_or(false);
                    Some(HostBattery::new(
                        p,
                        chg,
                        chg,
                        if chg { "Charging" } else { "Discharging" },
                    ))
                } else if let Some(mut b) = current_bat {
                    if let Some(c) = charging {
                        b.is_charging = c;
                    }
                    Some(b)
                } else {
                    None
                };

                let default_dur = st.config.default_duration;
                let critical_threshold = st.config.laptop_battery.critical_threshold;
                let should_restore_clock = st.is_battery_override
                    && bat_state.as_ref().is_none_or(|b| {
                        !b.should_override_clock_with_threshold(critical_threshold)
                    });
                if should_restore_clock {
                    st.is_battery_override = false;
                }

                let dev_opt = if should_restore_clock {
                    st.device.clone()
                } else {
                    None
                };

                if display && let Some(ref b) = bat_state {
                    let item = NotificationItem {
                        message: format!("Battery: {}%", b.percent),
                        title: Some("Battery".into()),
                        icon: None,
                        canvas: Some(CanvasType::Battery {
                            percent: b.percent,
                            charging: b.is_charging,
                            is_low: b.is_low(),
                        }),
                        duration: duration.unwrap_or(default_dur),
                        priority: Priority::Normal,
                        sound: false,
                        created_at: Instant::now(),
                    };
                    st.enqueue(item);
                }
                drop(st);

                if let Some(dev) = dev_opt {
                    let _ = dev.show_clock().await;
                }

                if display && bat_state.is_some() {
                    let _ = tx_notify.send(()).await;
                }

                let resp = serde_json::json!({
                    "status": "ok",
                    "battery": bat_state,
                });
                writer.write_all(format!("{resp}\n").as_bytes()).await?;
            }
            Command::Sysmon { display, duration } => {
                let snapshot = sysmon::read_instant_snapshot();
                let mut st = state.lock().await;
                let default_dur = st.config.default_duration;

                if display {
                    let item = NotificationItem {
                        message: format!(
                            "CPU: {:.0}% RAM: {:.0}%",
                            snapshot.cpu_pct, snapshot.ram_pct
                        ),
                        title: Some("Sysmon".into()),
                        icon: None,
                        canvas: Some(CanvasType::Sysmon {
                            cpu: snapshot.cpu_pct as u32,
                            ram: snapshot.ram_pct as u32,
                        }),
                        duration: duration.unwrap_or(default_dur),
                        priority: Priority::Normal,
                        sound: false,
                        created_at: Instant::now(),
                    };
                    st.enqueue(item);
                }
                drop(st);

                if display {
                    let _ = tx_notify.send(()).await;
                }

                let resp = serde_json::json!({
                    "status": "ok",
                    "sysmon": snapshot,
                });
                writer.write_all(format!("{resp}\n").as_bytes()).await?;
            }
            Command::Clock => {
                let dev = ensure_device(&state).await;
                if let Some(dev) = dev {
                    let res = dev.show_clock().await;
                    let resp = match res {
                        Ok(_) => serde_json::json!({"status": "ok"}),
                        Err(e) => serde_json::json!({"status": "error", "message": e}),
                    };
                    writer.write_all(format!("{resp}\n").as_bytes()).await?;
                } else {
                    let resp =
                        serde_json::json!({"status": "error", "message": "Device not connected"});
                    writer.write_all(format!("{resp}\n").as_bytes()).await?;
                }
            }
            Command::SyncTime => {
                let dev = ensure_device(&state).await;
                if let Some(dev) = dev {
                    let res = dev.sync_time().await;
                    let resp = match res {
                        Ok(_) => serde_json::json!({"status": "ok"}),
                        Err(e) => serde_json::json!({"status": "error", "message": e}),
                    };
                    writer.write_all(format!("{resp}\n").as_bytes()).await?;
                } else {
                    let resp =
                        serde_json::json!({"status": "error", "message": "Device not connected"});
                    writer.write_all(format!("{resp}\n").as_bytes()).await?;
                }
            }
            Command::Brightness { level } => {
                let dev = ensure_device(&state).await;
                if let Some(dev) = dev {
                    let res = dev.set_brightness(level).await;
                    let resp = match res {
                        Ok(_) => serde_json::json!({"status": "ok"}),
                        Err(e) => serde_json::json!({"status": "error", "message": e}),
                    };
                    writer.write_all(format!("{resp}\n").as_bytes()).await?;
                } else {
                    let resp =
                        serde_json::json!({"status": "error", "message": "Device not connected"});
                    writer.write_all(format!("{resp}\n").as_bytes()).await?;
                }
            }
            Command::Screen { on } => {
                let dev = ensure_device(&state).await;
                if let Some(dev) = dev {
                    let res = dev.set_screen_power(on).await;
                    let resp = match res {
                        Ok(_) => serde_json::json!({"status": "ok"}),
                        Err(e) => serde_json::json!({"status": "error", "message": e}),
                    };
                    writer.write_all(format!("{resp}\n").as_bytes()).await?;
                } else {
                    let resp =
                        serde_json::json!({"status": "error", "message": "Device not connected"});
                    writer.write_all(format!("{resp}\n").as_bytes()).await?;
                }
            }
            Command::Status => {
                let dev = ensure_device(&state).await;
                let (battery_state, is_override) = {
                    let mut st = state.lock().await;
                    st.laptop_battery = battery::get_host_battery();
                    (st.laptop_battery.clone(), st.is_battery_override)
                };
                if let Some(dev) = dev {
                    let stat = dev.get_status().await.ok();
                    let resp = serde_json::json!({
                        "status": "ok",
                        "device": stat,
                        "battery": battery_state,
                        "is_battery_override": is_override,
                    });
                    writer.write_all(format!("{resp}\n").as_bytes()).await?;
                } else {
                    let resp = serde_json::json!({
                        "status": "ok",
                        "device": null,
                        "battery": battery_state,
                        "is_battery_override": is_override,
                        "message": "Not connected",
                    });
                    writer.write_all(format!("{resp}\n").as_bytes()).await?;
                }
            }
        }
    }

    Ok(())
}

async fn ensure_device(state: &Arc<Mutex<DaemonState>>) -> Option<DeviceHandle> {
    let mac = {
        let st = state.lock().await;
        if st.device.is_some() {
            return st.device.clone();
        }
        st.config.device_mac.clone()
    };

    if let Ok((handle, addr)) = device::spawn_device_worker(mac).await {
        let mut st = state.lock().await;
        st.device = Some(handle.clone());
        st.device_addr = Some(addr);
        return Some(handle);
    }
    None
}

async fn worker_loop(state: Arc<Mutex<DaemonState>>, rx_notify: &mut mpsc::Receiver<()>) {
    loop {
        let item = {
            let mut st = state.lock().await;
            st.queue.pop_front()
        };

        if let Some(item) = item {
            process_notification(&item, &state).await;
        } else {
            // Restore clock OR battery critical alert when queue becomes empty
            let (dev_opt, should_override, bat_info) = {
                let mut st = state.lock().await;
                if st.is_displaying {
                    st.is_displaying = false;
                }
                st.laptop_battery = battery::get_host_battery();
                let critical_threshold = st.config.laptop_battery.critical_threshold;
                let should_override = st
                    .laptop_battery
                    .as_ref()
                    .is_some_and(|b| b.should_override_clock_with_threshold(critical_threshold));
                st.is_battery_override = should_override;
                (
                    st.device.clone(),
                    should_override,
                    st.laptop_battery.clone(),
                )
            };

            if let Some(dev) = dev_opt {
                if should_override && let Some(bat) = bat_info {
                    log::warn!(
                        "Queue empty, but laptop battery is critical (<10% and not charging). Showing battery icon instead of clock."
                    );
                    let canvas = Canvas::render_battery(bat.percent, bat.is_charging, true);
                    if let Ok(temp_path) = canvas.to_temp_png() {
                        let _ = dev.show_image(temp_path.clone()).await;
                        let _ = fs::remove_file(temp_path);
                    }
                } else {
                    log::info!("Queue empty; restoring native clock...");
                    let _ = dev.show_clock().await;
                }
            }

            // Wait for next notification
            let _ = rx_notify.recv().await;
        }
    }
}

async fn process_notification(item: &NotificationItem, state: &Arc<Mutex<DaemonState>>) {
    log::info!(
        "Displaying notification: '{}' (icon: {:?}, priority: {:?})",
        item.message,
        item.icon,
        item.priority
    );

    let (dev, sound_cmd) = {
        let mut st = state.lock().await;
        st.is_displaying = true;
        (st.device.clone(), st.config.sound.command.clone())
    };

    let dev = match dev {
        Some(d) => Some(d),
        None => ensure_device(state).await,
    };

    let Some(dev) = dev else {
        log::error!("Cannot display notification: Ditoo Pro device handle not initialized.");
        return;
    };

    // Play sound if requested
    if item.sound {
        let cmd = sound_cmd;
        tokio::spawn(async move {
            let parts: Vec<&str> = cmd.split_whitespace().collect();
            if let Some((bin, args)) = parts.split_first() {
                let _ = tokio::process::Command::new(bin).args(args).output().await;
            }
        });
    }

    // Check if this item has a dynamic canvas widget
    if let Some(ref canvas_type) = item.canvas {
        match canvas_type.render() {
            Ok(canvas) => {
                if let Ok(temp_path) = canvas.to_temp_png() {
                    if let Err(e) = dev.show_image(temp_path.clone()).await {
                        log::warn!("Failed to show dynamic canvas: {}", e);
                    }
                    let _ = fs::remove_file(temp_path);
                }
            }
            Err(e) => {
                log::warn!("Failed to render dynamic canvas: {}", e);
            }
        }

        let dur = Duration::from_secs_f64(item.duration.max(1.0));
        tokio::time::sleep(dur).await;
        return;
    }

    // Display icon or text
    let mut displayed = false;
    if let Some(icon_name) = &item.icon
        && let Some(resolved) = icons::resolve_icon(icon_name)
    {
        match resolved.display(&dev).await {
            Ok(_) => {
                displayed = true;
            }
            Err(e) => {
                log::warn!("Failed to display resolved icon '{icon_name}': {e}");
            }
        }
    }

    if !displayed {
        // Fall back to static text display
        let color = match item.priority {
            Priority::Urgent | Priority::High => "red",
            Priority::Low => "cyan",
            Priority::Normal => "green",
        };
        let text: String = item.message.chars().take(6).collect();
        if let Err(e) = dev.show_text(text, color.to_string()).await {
            log::warn!("Failed to show text: {}", e);
        }
    }

    // Hold notification for specified duration
    let dur = Duration::from_secs_f64(item.duration.max(1.0));
    tokio::time::sleep(dur).await;
}

fn is_quiet_hours(config: &Config) -> bool {
    if !config.quiet_hours.enabled {
        return false;
    }
    let now = chrono::Local::now().time();
    let start = chrono::NaiveTime::parse_from_str(&config.quiet_hours.start, "%H:%M").ok();
    let end = chrono::NaiveTime::parse_from_str(&config.quiet_hours.end, "%H:%M").ok();

    if let (Some(s), Some(e)) = (start, end) {
        if s <= e {
            now >= s && now <= e
        } else {
            now >= s || now <= e
        }
    } else {
        false
    }
}
