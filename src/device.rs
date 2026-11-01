use std::fs::File;
use std::path::PathBuf;
use std::str::FromStr;
use bluer::Address;
use divoom_ditoo_pro_controller::{
    find_paired_ditoo_pro_devices, send_divoom_animation, send_get_clock_face,
    send_get_volume, send_image, send_set_box_mode, send_set_brightness,
    send_set_datetime, send_set_screen_power, send_static_text,
};
use divoom_ditoo_pro_controller::protocol::scrolling_text::{HAlign, VAlign};
use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, oneshot};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceStatus {
    pub address: String,
    pub connected: bool,
    pub volume: Option<u8>,
    pub clock_id: Option<u16>,
}

pub enum DeviceRequest {
    ShowClock {
        reply: oneshot::Sender<Result<(), String>>,
    },
    SyncTime {
        reply: oneshot::Sender<Result<(), String>>,
    },
    SetScreenPower {
        on: bool,
        reply: oneshot::Sender<Result<(), String>>,
    },
    SetBrightness {
        level: u8,
        reply: oneshot::Sender<Result<(), String>>,
    },
    ShowImage {
        path: PathBuf,
        reply: oneshot::Sender<Result<(), String>>,
    },
    ShowAnimation {
        path: PathBuf,
        reply: oneshot::Sender<Result<(), String>>,
    },
    ShowText {
        text: String,
        color: String,
        reply: oneshot::Sender<Result<(), String>>,
    },
    GetStatus {
        reply: oneshot::Sender<Result<DeviceStatus, String>>,
    },
}

#[derive(Clone)]
pub struct DeviceHandle {
    tx: mpsc::Sender<DeviceRequest>,
}

impl DeviceHandle {
    pub async fn show_clock(&self) -> Result<(), String> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(DeviceRequest::ShowClock { reply })
            .await
            .map_err(|e| e.to_string())?;
        rx.await.map_err(|e| e.to_string())?
    }

    pub async fn sync_time(&self) -> Result<(), String> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(DeviceRequest::SyncTime { reply })
            .await
            .map_err(|e| e.to_string())?;
        rx.await.map_err(|e| e.to_string())?
    }

    pub async fn set_screen_power(&self, on: bool) -> Result<(), String> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(DeviceRequest::SetScreenPower { on, reply })
            .await
            .map_err(|e| e.to_string())?;
        rx.await.map_err(|e| e.to_string())?
    }

    pub async fn set_brightness(&self, level: u8) -> Result<(), String> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(DeviceRequest::SetBrightness { level, reply })
            .await
            .map_err(|e| e.to_string())?;
        rx.await.map_err(|e| e.to_string())?
    }

    pub async fn show_image(&self, path: PathBuf) -> Result<(), String> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(DeviceRequest::ShowImage { path, reply })
            .await
            .map_err(|e| e.to_string())?;
        rx.await.map_err(|e| e.to_string())?
    }

    pub async fn show_animation(&self, path: PathBuf) -> Result<(), String> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(DeviceRequest::ShowAnimation { path, reply })
            .await
            .map_err(|e| e.to_string())?;
        rx.await.map_err(|e| e.to_string())?
    }

    pub async fn show_text(&self, text: String, color: String) -> Result<(), String> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(DeviceRequest::ShowText { text, color, reply })
            .await
            .map_err(|e| e.to_string())?;
        rx.await.map_err(|e| e.to_string())?
    }

    pub async fn get_status(&self) -> Result<DeviceStatus, String> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(DeviceRequest::GetStatus { reply })
            .await
            .map_err(|e| e.to_string())?;
        rx.await.map_err(|e| e.to_string())?
    }
}

pub async fn spawn_device_worker(mac_override: Option<String>) -> Result<(DeviceHandle, Address), String> {
    let addr = resolve_device(mac_override.as_deref()).await?;

    let (tx, mut rx) = mpsc::channel::<DeviceRequest>(32);
    let handle = DeviceHandle { tx };

    std::thread::Builder::new()
        .name("ditoo-worker".into())
        .spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("Failed to build current_thread runtime for Bluetooth worker");

            rt.block_on(async move {
                log::debug!("Bluetooth worker started for device {}", addr);
                while let Some(req) = rx.recv().await {
                    match req {
                        DeviceRequest::ShowClock { reply } => {
                            // Ensure screen power is on
                            let _ = send_set_screen_power(addr, true).await;
                            // Set channel to 0x00 (Time / Native Clock Channel) with green font (0x00, 0xFF, 0x00)
                            let _ = send_set_box_mode(addr, vec![0x00]).await;
                            let payload = vec![0x00, 0x01, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0xff, 0x00];
                            let res = send_set_box_mode(addr, payload).await.map_err(|e| e.to_string());
                            let _ = reply.send(res);
                        }
                        DeviceRequest::SyncTime { reply } => {
                            let now = chrono::Local::now().naive_local();
                            let res = send_set_datetime(addr, now).await.map_err(|e| e.to_string());
                            let _ = reply.send(res);
                        }
                        DeviceRequest::SetScreenPower { on, reply } => {
                            let res = send_set_screen_power(addr, on).await.map_err(|e| e.to_string());
                            let _ = reply.send(res);
                        }
                        DeviceRequest::SetBrightness { level, reply } => {
                            let res = send_set_brightness(addr, level).await.map_err(|e| e.to_string());
                            let _ = reply.send(res);
                        }
                        DeviceRequest::ShowImage { path, reply } => {
                            let _ = send_set_screen_power(addr, true).await;
                            let res = match path.to_str() {
                                Some(s) => send_image(addr, s).await.map_err(|e| e.to_string()),
                                None => Err("Invalid UTF-8 in image path".to_string()),
                            };
                            let _ = reply.send(res);
                        }
                        DeviceRequest::ShowAnimation { path, reply } => {
                            let _ = send_set_screen_power(addr, true).await;
                            let res = match File::open(&path) {
                                Ok(mut f) => send_divoom_animation(addr, &mut f).await.map_err(|e| e.to_string()),
                                Err(e) => Err(e.to_string()),
                            };
                            let _ = reply.send(res);
                        }
                        DeviceRequest::ShowText { text, color, reply } => {
                            let _ = send_set_screen_power(addr, true).await;
                            let font_path = find_default_font();
                            let res = match parse_color(&color) {
                                Ok(fg_color) => {
                                    send_static_text(
                                        addr,
                                        &font_path,
                                        &text,
                                        16.0,
                                        fg_color,
                                        [0, 0, 0],
                                        HAlign::Center,
                                        VAlign::Center,
                                    )
                                    .await
                                    .map_err(|e| e.to_string())
                                }
                                Err(e) => Err(e),
                            };
                            let _ = reply.send(res);
                        }
                        DeviceRequest::GetStatus { reply } => {
                            let volume = send_get_volume(addr).await.ok();
                            let clock_id = send_get_clock_face(addr).await.ok();
                            let _ = reply.send(Ok(DeviceStatus {
                                address: addr.to_string(),
                                connected: true,
                                volume,
                                clock_id,
                            }));
                        }
                    }
                }
                log::debug!("Bluetooth worker stopped.");
            });
        })
        .map_err(|e| e.to_string())?;

    Ok((handle, addr))
}

pub async fn resolve_device(configured_mac: Option<&str>) -> Result<Address, String> {
    if let Some(mac) = configured_mac
        && let Ok(addr) = Address::from_str(mac) {
            return Ok(addr);
        }

    if let Ok(devices) = find_paired_ditoo_pro_devices().await
        && let Some(&first) = devices.first() {
            return Ok(first);
        }

    // Fallback: check bluetoothctl devices for any paired device matching ditoo/divoom
    if let Ok(output) = tokio::process::Command::new("bluetoothctl")
        .args(["devices"])
        .output()
        .await
        && output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            for line in stdout.lines() {
                let lower = line.to_lowercase();
                if lower.contains("ditoo") || lower.contains("divoom") {
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    if parts.len() >= 2
                        && let Ok(addr) = Address::from_str(parts[1]) {
                            log::info!("Discovered paired Ditoo via bluetoothctl fallback: {addr}");
                            return Ok(addr);
                        }
                }
            }
        }

    Err("No paired Ditoo Pro device found. Please verify pairing with bluetoothctl.".to_string())
}

pub fn find_default_font() -> PathBuf {
    let candidates = [
        "/usr/share/fonts/noto/NotoSansMono-Regular.ttf",
        "/usr/share/fonts/TTF/DejaVuSans.ttf",
        "/usr/share/fonts/TTF/Inconsolata-Bold.ttf",
        "/usr/share/fonts/TTF/JetBrainsMonoNerdFontPropo-Bold.ttf",
    ];
    for c in candidates {
        let p = PathBuf::from(c);
        if p.exists() {
            return p;
        }
    }
    if let Ok(output) = std::process::Command::new("fc-match")
        .args(["monospace", "--format=%{file}"])
        .output()
        && output.status.success()
            && let Ok(s) = String::from_utf8(output.stdout) {
                let p = PathBuf::from(s.trim());
                if p.exists() {
                    return p;
                }
            }
    PathBuf::from("/usr/share/fonts/TTF/DejaVuSans.ttf")
}

fn parse_color(s: &str) -> Result<[u8; 3], String> {
    let c = csscolorparser::parse(s).map_err(|e| format!("Invalid color '{s}': {e}"))?;
    let [r, g, b, _] = c.to_rgba8();
    Ok([r, g, b])
}
