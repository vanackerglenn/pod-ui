//! In-process POD Go setlist (preset-name) reader.
//!
//! This is a faithful in-process port of the `podgo_probe` example: it performs
//! the x1 setlist handshake, opens the preset resource, pages the MessagePack
//! stream, and extracts each preset's name (map field 109). It replaces the
//! previous approach of shelling out to the `podgo_probe` example binary so the
//! functionality lives in the module proper and no longer depends on the example
//! being built.
//!
//! The byte sequences below are copied verbatim from the working probe; do not
//! "tidy" them without a matching hardware capture — they are the device's exact
//! expected packets.

use log::*;
use rusb::{Context, UsbContext};

const VID: u16 = 0x0e41;
const PID: u16 = 0x4247;
const EP_OUT: u8 = 0x01;
const EP_IN: u8 = 0x81;
const TIMEOUT: std::time::Duration = std::time::Duration::from_secs(2);
const DRAIN_TIMEOUT: std::time::Duration = std::time::Duration::from_millis(50);
const MAX_CHUNK: usize = 272;

// --- x1 setlist handshake packets (verbatim from podgo_probe) ---

const HANDSHAKE: [u8; 20] = [
    0x0C, 0x00, 0x00, 0x28, 0x01, 0x10, 0xEF, 0x03,
    0x00, 0x00, 0x00, 0x02, 0x00, 0x01, 0x00, 0x21,
    0x00, 0x10, 0x00, 0x00,
];

const SESSION_OPEN_1: [u8; 28] = [
    0x11, 0x00, 0x00, 0x18, 0x01, 0x10, 0xEF, 0x03,
    0x00, 0x02, 0x00, 0x04, 0x00, 0x10, 0x00, 0x00,
    0x01, 0x00, 0x02, 0x00, 0x01, 0x00, 0x00, 0x00,
    0x02, 0x00, 0x00, 0x00,
];

const SESSION_CHUNK_1: [u8; 16] = [
    0x08, 0x00, 0x00, 0x18, 0x01, 0x10, 0xEF, 0x03,
    0x00, 0x03, 0x00, 0x08, 0x09, 0x10, 0x00, 0x00,
];

const SESSION_OPEN_2: [u8; 36] = [
    0x1A, 0x00, 0x00, 0x18, 0x01, 0x10, 0xEF, 0x03,
    0x00, 0x04, 0x00, 0x04, 0x09, 0x10, 0x00, 0x00,
    0x01, 0x00, 0x02, 0x00, 0x0A, 0x00, 0x00, 0x00,
    0x83, 0x66, 0xCD, 0x03, 0xE8, 0x64, 0xCC, 0xFE,
    0x65, 0x80, 0x00, 0x00,
];

const SESSION_CHUNK_2: [u8; 16] = [
    0x08, 0x00, 0x00, 0x18, 0x01, 0x10, 0xEF, 0x03,
    0x00, 0x05, 0x00, 0x08, 0x1A, 0x10, 0x00, 0x00,
];

const OPEN_PRESETS: [u8; 36] = [
    0x19, 0x00, 0x00, 0x18, 0x01, 0x10, 0xEF, 0x03,
    0x00, 0x06, 0x00, 0x04, 0x1A, 0x10, 0x00, 0x00,
    0x01, 0x00, 0x02, 0x00, 0x09, 0x00, 0x00, 0x00,
    0x83, 0x66, 0xCD, 0x03, 0xE9, 0x64, 0x00, 0x65,
    0xC0, 0x00, 0x00, 0x00,
];

const OPEN_STREAM: [u8; 40] = [
    0x1D, 0x00, 0x00, 0x18, 0x01, 0x10, 0xEF, 0x03,
    0x00, 0x07, 0x00, 0x0C, 0x38, 0x10, 0x00, 0x00,
    0x01, 0x00, 0x02, 0x00, 0x0D, 0x00, 0x00, 0x00,
    0x83, 0x66, 0xCD, 0x03, 0xEA, 0x64, 0x01, 0x65,
    0x82, 0x6B, 0x00, 0x65, 0x02, 0x00, 0x00, 0x00,
];

/// Streamed-chunk requester: 16-byte header carrying the running seq and offset.
fn chunk_request(seq: u8, offset: u32) -> [u8; 16] {
    let off = offset.to_le_bytes();
    [
        0x08, 0x00, 0x00, 0x18, 0x01, 0x10, 0xEF, 0x03,
        0x00, seq, 0x00, 0x08, off[0], off[1], off[2], off[3],
    ]
}

/// Read the device's setlist and return `(preset_index, name)` pairs.
///
/// Opens the device, claims interface 0, runs the setlist handshake, pages the
/// preset stream, parses the names, and releases the interface. One-shot: the
/// handle is dropped on return.
pub fn fetch_preset_names() -> anyhow::Result<Vec<(u16, String)>> {
    let ctx = Context::new()?;
    let devices = ctx.devices()?;

    let dev = devices.iter().find(|dev| {
        dev.device_descriptor().map_or(false, |d| {
            d.vendor_id() == VID && d.product_id() == PID
        })
    }).ok_or_else(|| anyhow::anyhow!("POD Go not found"))?;

    let handle = dev.open()?;

    let iface = 0u8;
    let _ = handle.set_auto_detach_kernel_driver(true);
    handle.claim_interface(iface)
        .map_err(|e| anyhow::anyhow!("Cannot claim interface 0: {}", e))?;

    let _ = handle.clear_halt(EP_OUT);
    let _ = handle.clear_halt(EP_IN);

    drain_stale(&handle);
    do_handshake(&handle)?;
    let stream = list_presets(&handle)?;

    let _ = handle.release_interface(iface);

    Ok(parse_preset_names(&stream))
}

fn drain_stale(handle: &rusb::DeviceHandle<Context>) {
    let mut buf = [0u8; 512];
    loop {
        match handle.read_bulk(EP_IN, &mut buf, DRAIN_TIMEOUT) {
            Ok(n) => trace!("  drain: read {} bytes", n),
            Err(rusb::Error::Timeout) => break,
            Err(e) => {
                warn!("  drain error: {}", e);
                break;
            }
        }
    }
}

fn write_read(handle: &rusb::DeviceHandle<Context>, data: &[u8], buf: &mut [u8]) -> anyhow::Result<usize> {
    handle.write_bulk(EP_OUT, data, TIMEOUT)?;
    let n = handle.read_bulk(EP_IN, buf, TIMEOUT)?;
    Ok(n)
}

/// Phase 0: the 5-packet x1 session handshake.
fn do_handshake(handle: &rusb::DeviceHandle<Context>) -> anyhow::Result<()> {
    let mut buf = [0u8; 512];
    write_read(handle, &HANDSHAKE, &mut buf)?;
    write_read(handle, &SESSION_OPEN_1, &mut buf)?;
    write_read(handle, &SESSION_CHUNK_1, &mut buf)?;
    write_read(handle, &SESSION_OPEN_2, &mut buf)?;
    write_read(handle, &SESSION_CHUNK_2, &mut buf)?;
    Ok(())
}

/// Phases 1-3: open the preset resource, start the paged stream, and collect
/// every chunk until a short read signals the end.
fn list_presets(handle: &rusb::DeviceHandle<Context>) -> anyhow::Result<Vec<u8>> {
    let mut buf = [0u8; 512];

    // Phase 1: open preset resource.
    write_read(handle, &OPEN_PRESETS, &mut buf)?;

    // Phase 2: start the paged stream (chunk #0).
    let mut stream = Vec::new();
    let n = write_read(handle, &OPEN_STREAM, &mut buf)?;
    if n > 16 {
        stream.extend_from_slice(&buf[16..n]);
    }

    // Phase 3: chunk loop. seq/offset continue from the probe's known-good values.
    let mut seq: u8 = 0x08;
    let mut offset: u32 = 0x0000_1138;
    loop {
        let req = chunk_request(seq, offset);
        let n = write_read(handle, &req, &mut buf)?;
        if n > 16 {
            stream.extend_from_slice(&buf[16..n]);
        }
        if n < MAX_CHUNK {
            // short read -> end of stream
            break;
        }
        offset += 0x0100;
        seq = seq.wrapping_add(1);
    }

    trace!("setlist stream size: {} bytes", stream.len());
    Ok(stream)
}

/// Parse the MessagePack preset array, extracting `(index, name)` per entry.
/// The names live in map field 109; everything else is skipped.
fn parse_preset_names(data: &[u8]) -> Vec<(u16, String)> {
    // Find the array16(128) marker that begins the preset list.
    let marker = [0xDC, 0x00, 0x80];
    let Some(start) = data.windows(3).position(|w| w == marker) else {
        warn!("setlist array marker (DC 00 80) not found");
        return Vec::new();
    };

    let count = u16::from_be_bytes([data[start + 1], data[start + 2]]) as usize;
    let mut pos = start + 3;
    let mut names = Vec::with_capacity(count);

    for _ in 0..count {
        if pos >= data.len() {
            break;
        }
        // Each entry is fixmap(1): { index: { ...fields... } }
        if data[pos] != 0x81 {
            break;
        }
        pos += 1;

        let (key, klen) = decode_msgpack_int(data, pos);
        pos += klen;
        let index = key as u16;

        // Value: a map of preset fields.
        let map_byte = data[pos];
        let map_entries = if (map_byte & 0xF0) == 0x80 {
            let n = (map_byte & 0x0F) as usize;
            pos += 1;
            n
        } else if map_byte == 0xDE {
            let n = u16::from_be_bytes([data[pos + 1], data[pos + 2]]) as usize;
            pos += 3;
            n
        } else {
            break;
        };

        let mut name = String::new();
        for _ in 0..map_entries {
            let (field_key, fklen) = decode_msgpack_int(data, pos);
            pos += fklen;
            if field_key == 109 {
                let (val, vlen) = decode_msgpack_str(data, pos);
                pos += vlen;
                name = val.trim_end_matches('\0').to_string();
            } else {
                pos += skip_msgpack_value(data, pos);
            }
        }

        names.push((index, name));
    }

    names
}

fn decode_msgpack_int(data: &[u8], pos: usize) -> (u64, usize) {
    if pos >= data.len() { return (0, 1); }
    let b = data[pos];
    if b <= 0x7F {
        (b as u64, 1)
    } else if b == 0xCC && pos + 1 < data.len() {
        (data[pos + 1] as u64, 2)
    } else if b == 0xCD && pos + 2 < data.len() {
        (u16::from_be_bytes([data[pos + 1], data[pos + 2]]) as u64, 3)
    } else if b == 0xCE && pos + 4 < data.len() {
        (u32::from_be_bytes([data[pos + 1], data[pos + 2], data[pos + 3], data[pos + 4]]) as u64, 5)
    } else {
        (0, 1)
    }
}

fn decode_msgpack_str(data: &[u8], pos: usize) -> (String, usize) {
    if pos >= data.len() { return (String::new(), 1); }
    let b = data[pos];
    let (len, hdr) = if (b & 0xE0) == 0xA0 {
        ((b & 0x1F) as usize, 1)
    } else if b == 0xD9 && pos + 1 < data.len() {
        (data[pos + 1] as usize, 2)
    } else if b == 0xDA && pos + 2 < data.len() {
        (u16::from_be_bytes([data[pos + 1], data[pos + 2]]) as usize, 3)
    } else {
        return (String::new(), 1);
    };
    let end = (pos + hdr + len).min(data.len());
    let s = String::from_utf8_lossy(&data[pos + hdr..end]).to_string();
    (s, hdr + len)
}

fn skip_msgpack_value(data: &[u8], pos: usize) -> usize {
    if pos >= data.len() { return 1; }
    let b = data[pos];
    match b {
        0xC0 | 0xC2 | 0xC3 => 1,
        0xCC => 2,
        0xCD => 3,
        0xCE => 5,
        0xCA => 5,
        0xCB => 9,
        0xD9 if pos + 1 < data.len() => 2 + data[pos + 1] as usize,
        0xDA if pos + 2 < data.len() => 3 + u16::from_be_bytes([data[pos + 1], data[pos + 2]]) as usize,
        0xDB if pos + 4 < data.len() => 5 + u32::from_be_bytes([data[pos + 1], data[pos + 2], data[pos + 3], data[pos + 4]]) as usize,
        0xC4 if pos + 1 < data.len() => 2 + data[pos + 1] as usize,
        0xC5 if pos + 2 < data.len() => 3 + u16::from_be_bytes([data[pos + 1], data[pos + 2]]) as usize,
        0xDC if pos + 2 < data.len() => {
            let n = u16::from_be_bytes([data[pos + 1], data[pos + 2]]) as usize;
            3 + skip_n_values(data, pos + 3, n)
        }
        0xDE if pos + 2 < data.len() => {
            let n = u16::from_be_bytes([data[pos + 1], data[pos + 2]]) as usize;
            3 + skip_n_map_entries(data, pos + 3, n)
        }
        _ if b <= 0x7F => 1,
        _ if (b & 0xE0) == 0xA0 => 1 + (b & 0x1F) as usize,
        _ if (b & 0xF0) == 0x80 => 1 + skip_n_map_entries(data, pos + 1, (b & 0x0F) as usize),
        _ if (b & 0xF0) == 0x90 => 1 + skip_n_values(data, pos + 1, (b & 0x0F) as usize),
        _ => 1,
    }
}

fn skip_n_values(data: &[u8], mut pos: usize, n: usize) -> usize {
    let start = pos;
    for _ in 0..n {
        pos += skip_msgpack_value(data, pos);
    }
    pos - start
}

fn skip_n_map_entries(data: &[u8], mut pos: usize, n: usize) -> usize {
    let start = pos;
    for _ in 0..n {
        pos += skip_msgpack_value(data, pos);
        pos += skip_msgpack_value(data, pos);
    }
    pos - start
}
