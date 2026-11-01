# Divoom Ditoo Pro — Bluetooth RFCOMM Protocol Specification

This document details the reverse-engineered binary communication protocol used to control the Divoom Ditoo Pro over Bluetooth Classic Serial Port Profile (SPP / RFCOMM).

Protocol research and packet envelope specifications are based on the reverse-engineering work by [**andreas-mausch**](https://github.com/andreas-mausch/divoom-ditoo-pro-controller) and the enhanced implementation by [**futpib**](https://github.com/futpib/divoom-ditoo-pro-controller).

---

## 1. Frame Structure

Every command sent to or received from the Ditoo Pro follows a uniform binary envelope:

| Offset | Field | Type | Description |
|---|---|---|---|
| `0` | Start Byte | `u8` | Always `0x01` |
| `1..2` | Length | `u16` (LE) | Little-endian payload length + 3 (Command byte + 2 Checksum bytes) |
| `3` | Command ID | `u8` | Opcode defining the action |
| `4..N` | Payload | `[u8]` | Variable command-specific arguments |
| `N+1..N+2` | Checksum | `u16` (LE) | Little-endian sum of bytes: `sum(Length) + Command + sum(Payload) mod 65536` |
| `N+3` | End Byte | `u8` | Always `0x02` |

### Checksum Algorithm
```rust
fn calculate_checksum(length_le: [u8; 2], command: u8, payload: &[u8]) -> u16 {
    let mut sum: u32 = (length_le[0] as u32) + (length_le[1] as u32) + (command as u32);
    for byte in payload {
        sum = sum.wrapping_add(*byte as u32);
    }
    (sum & 0xFFFF) as u16
}
```

---

## 2. Response Frames

Responses from the device follow the same envelope structure with Command ID `0x04`:

| Field | Type | Description |
|---|---|---|
| Response Opcode | `0x04` | Standard response marker |
| Original Command | `u8` | The command being acknowledged |
| Status / ACK | `u8` | `0x55` indicates ACK (success); `0x00` indicates NAK/error |
| Data | `[u8]` | Command-specific return values |

---

## 3. Implemented Commands

### 3.1 Date and Time Synchronization (`0x18`)
Synchronizes the internal RTC clock chip to match system time:
- **Command**: `0x18`
- **Payload** (7 bytes):
  - `[0]`: `year % 100` (e.g. `26` for 2026)
  - `[1]`: `year / 100` (e.g. `20` for 2026)
  - `[2]`: Month (`1..12`)
  - `[3]`: Day (`1..31`)
  - `[4]`: Hour (`0..23`)
  - `[5]`: Minute (`0..59`)
  - `[6]`: Second (`0..59`)

### 3.2 Display / Box Mode (`0x45`)
Switches the display between built-in native channels:
- **Command**: `0x45`
- **Native Time Channel (Clock) Payload**:
  - Direct Channel Switch: `[0x00]`
  - Detailed Format: `[0x00, 0x01, TT, XX, WW, EE, CC, RR, GG, BB]`
    - `0x00`: Channel 0 (Time)
    - `0x01`: Sub-channel options flag
    - `TT`: Clock face type (`0x00` Full screen, `0x01` Rainbow, `0x02` Box, `0x03` Analog Square, `0x05` Analog Round)
    - `XX`: Show time (`0x01`)
    - `WW`: Show weather (`0x00`)
    - `EE`: Show temperature (`0x00`)
    - `CC`: Show calendar (`0x00`)
    - `RR, GG, BB`: Digit color (e.g. `0x00, 0xFF, 0x00` for green digits)

### 3.3 Screen Power Control (`0xbd 0x2f`)
Turns the physical LED matrix on or off:
- **Extended Command**: `0xbd`
- **Prefix**: `0x2f` (`SPP_SECOND_OPEN_SCREEN_CTRL`)
- **Payload**: `[0x2f, power_on]` (`0x01` = ON, `0x00` = OFF)

### 3.4 Set Brightness (`0x74`)
Sets display brightness from 0 to 100%:
- **Command**: `0x74`
- **Payload** (1 byte): `brightness` (`0..=100`)

### 3.5 Image / Animation Streaming (`0x8b`)
Transfers 16×16 pixel art or multi-frame GIF animations:
- **Command**: `0x8b`
- **Control Words**:
  - `ControlWord::StartSeeding (0x00)`:
    - Payload: `[0x00, file_size_u32_le]`
  - `ControlWord::SendingData (0x01)`:
    - Payload: `[0x01, file_size_u32_le, chunk_offset_u16_le, data_bytes (up to 256)]`
  - `ControlWord::TerminateSending (0x02)`:
    - Payload: `[0x02]`

### 3.6 Audio Volume Query (`0x09`) & Set (`0x08`)
- **GetVolume**: `0x09` with empty payload; returns current volume level (`0..16`).
- **SetVolume**: `0x08` with payload `[volume_level]`.

### 3.7 Device Battery Query (`0x5f`) [Unsupported by Ditoo Pro Firmware]
In older Divoom devices, opcode `0x5f` queried internal battery level:
- **Command**: `0x5f`
- **Payload**: None (empty)
- **Note**: Stock Ditoo Pro firmware (`306007`) does not respond to `0x5f` and does not expose `org.bluez.Battery1`. Rather than simulating a device battery, the controller monitors the Linux host laptop's battery instead.

### 3.8 Clock Face ID (`0xbd`)
Extended command selector:
- `0xbd 0x15`: Query user-defined clock face ID.
- `0xbd 0x14 [clock_id_le]`: Set user-defined clock face ID.
