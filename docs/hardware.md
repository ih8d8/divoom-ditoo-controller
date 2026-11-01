# Divoom Ditoo Pro — Hardware Discovery & Linux Bluetooth Research

## 1. Hardware Overview

The **Divoom Ditoo Pro** is a retro-styled desktop companion featuring:
- **Display**: 16×16 RGB LED pixel matrix (256 programmable pixels).
- **Controls**: Mechanical keyboard switches with RGB backlight + spring-loaded joystick lever.
- **Audio**: 15W DSP-tuned Bluetooth speaker + bass reflex port + TF (microSD) card slot.
- **Connectivity**: Bluetooth 5.0 Dual Mode (Classic Bluetooth BR/EDR + Bluetooth Low Energy BLE) and USB-C.
- **RTC**: Internal Real-Time Clock maintaining time across display switches.

---

## 2. Arch Linux Bluetooth Discovery

On Arch Linux with BlueZ 5.79+, running `bluetoothctl devices` and `bluetoothctl info` identified the physical device:

```yaml
Device: 11:22:33:44:55:66 (public)
Name: DitooPro-Audio
Alias: DitooPro-Audio
Class: 0x002c0408 (Audio Headset)
Paired: yes
Bonded: yes
Connected: yes
```

### Exposed Bluetooth Profiles and UUIDs:
- **Serial Port Profile (SPP)**: `00001101-0000-1000-8000-00805f9b34fb` (Display & Command Protocol)
- **Audio Sink (A2DP)**: `0000110b-0000-1000-8000-00805f9b34fb` (Music and system audio)
- **A/V Remote Control (AVRCP)**: `0000110e-0000-1000-8000-00805f9b34fb` (Media controls)
- **Handsfree Profile (HFP)**: `0000111e-0000-1000-8000-00805f9b34fb`
- **Vendor Custom UUID**: `0000ab00-0000-1000-8000-00805f9b34fb`

---

## 3. Coexistence of Display Control and Bluetooth Audio

A critical finding of Phase 1 research:
1. **RFCOMM Channel Allocation**:
   - RFCOMM Channel 1 is occupied by PipeWire / BlueZ for the Handsfree/Headset audio profile (`[Errno 16] Device or resource busy`).
   - RFCOMM Channel 2 is dedicated to the **Serial Port Profile (SPP)** for display and device control (`Channel 2: CONNECTED`).
2. **Audio Coexistence**:
   - The Ditoo Pro's display command protocol runs entirely over RFCOMM (L2CAP channel) in parallel with standard A2DP audio streaming.
   - Sending display commands, animations, or clock modes **does not interrupt or disconnect audio playback**.
   - Sound notifications can be piped directly through standard Linux audio utilities (`pw-play`, `paplay`) while the display renders the notification icon simultaneously.

---

## 4. Permissions & Privilege Boundaries

- **Root Access**: **NOT required**. BlueZ provides user-level D-Bus APIs and user-accessible RFCOMM sockets for paired devices.
- **BlueZ Profile Registration**: The `bluer` crate registers a user-session client profile with UUID `00001101-0000-1000-8000-00805f9b34fb`. BlueZ completes SDP resolution and passes the active stream file descriptor directly to the application without elevated privileges.
- **Device Battery Interface**: Modern BlueZ automatically exposes `org.bluez.Battery1` on the device D-Bus object path when connected, allowing unprivileged percentage queries without active RFCOMM polling.
