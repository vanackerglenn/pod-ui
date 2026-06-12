use std::time::Duration;
use anyhow::{anyhow, bail, Result};
use rusb::{Context, UsbContext};

const VID: u16 = 0x0e41;
const PID: u16 = 0x4247;
const EP_OUT: u8 = 0x01;
const EP_IN: u8 = 0x81;
const TIMEOUT: Duration = Duration::from_secs(2);
const DRAIN_TIMEOUT: Duration = Duration::from_millis(50);
const MAX_CHUNK: usize = 272;

fn handshake_packet() -> [u8; 20] {
    [
        0x0C, 0x00, 0x00, 0x28, 0x01, 0x10, 0xEF, 0x03,
        0x00, 0x00, 0x00, 0x02, 0x00, 0x01, 0x00, 0x21,
        0x00, 0x10, 0x00, 0x00,
    ]
}

fn session_open_1() -> [u8; 28] {
    [
        0x11, 0x00, 0x00, 0x18, 0x01, 0x10, 0xEF, 0x03,
        0x00, 0x02, 0x00, 0x04, 0x00, 0x10, 0x00, 0x00,
        0x01, 0x00, 0x02, 0x00, 0x01, 0x00, 0x00, 0x00,
        0x02, 0x00, 0x00, 0x00,
    ]
}

fn session_chunk_1() -> [u8; 16] {
    [
        0x08, 0x00, 0x00, 0x18, 0x01, 0x10, 0xEF, 0x03,
        0x00, 0x03, 0x00, 0x08, 0x09, 0x10, 0x00, 0x00,
    ]
}

fn session_open_2() -> [u8; 36] {
    [
        0x1A, 0x00, 0x00, 0x18, 0x01, 0x10, 0xEF, 0x03,
        0x00, 0x04, 0x00, 0x04, 0x09, 0x10, 0x00, 0x00,
        0x01, 0x00, 0x02, 0x00, 0x0A, 0x00, 0x00, 0x00,
        0x83, 0x66, 0xCD, 0x03, 0xE8, 0x64, 0xCC, 0xFE,
        0x65, 0x80, 0x00, 0x00,
    ]
}

fn session_chunk_2() -> [u8; 16] {
    [
        0x08, 0x00, 0x00, 0x18, 0x01, 0x10, 0xEF, 0x03,
        0x00, 0x05, 0x00, 0x08, 0x1A, 0x10, 0x00, 0x00,
    ]
}

fn open_presets() -> [u8; 36] {
    [
        0x19, 0x00, 0x00, 0x18, 0x01, 0x10, 0xEF, 0x03,
        0x00, 0x06, 0x00, 0x04, 0x1A, 0x10, 0x00, 0x00,
        0x01, 0x00, 0x02, 0x00, 0x09, 0x00, 0x00, 0x00,
        0x83, 0x66, 0xCD, 0x03, 0xE9, 0x64, 0x00, 0x65,
        0xC0, 0x00, 0x00, 0x00,
    ]
}

fn open_stream() -> [u8; 40] {
    [
        0x1D, 0x00, 0x00, 0x18, 0x01, 0x10, 0xEF, 0x03,
        0x00, 0x07, 0x00, 0x0C, 0x38, 0x10, 0x00, 0x00,
        0x01, 0x00, 0x02, 0x00, 0x0D, 0x00, 0x00, 0x00,
        0x83, 0x66, 0xCD, 0x03, 0xEA, 0x64, 0x01, 0x65,
        0x82, 0x6B, 0x00, 0x65, 0x02, 0x00, 0x00, 0x00,
    ]
}

fn chunk_request(seq: u8, offset: u32) -> [u8; 16] {
    let off = offset.to_le_bytes();
    [
        0x08, 0x00, 0x00, 0x18, 0x01, 0x10, 0xEF, 0x03,
        0x00, seq, 0x00, 0x08, off[0], off[1], off[2], off[3],
    ]
}

fn write_read(handle: &rusb::DeviceHandle<Context>, data: &[u8], buf: &mut [u8]) -> Result<usize> {
    let n = handle.write_bulk(EP_OUT, data, TIMEOUT)?;
    println!("  wrote {} bytes", n);
    let n = handle.read_bulk(EP_IN, buf, TIMEOUT)?;
    println!("  read  {} bytes", n);
    Ok(n)
}

fn drain(handle: &rusb::DeviceHandle<Context>) -> Result<()> {
    let mut buf = [0u8; 512];
    loop {
        match handle.read_bulk(EP_IN, &mut buf, DRAIN_TIMEOUT) {
            Ok(n) => { println!("  drain: read {} bytes", n); }
            Err(rusb::Error::Timeout) => { break; }
            Err(e) => { bail!("Drain error: {}", e); }
        }
    }
    println!("Drain complete");
    Ok(())
}

fn main() -> Result<()> {
    println!("=== POD Go HX Protocol Probe ===");
    println!("Opening POD Go (VID={:04X}, PID={:04X})...", VID, PID);
    let ctx = Context::new()?;

    let (handle, desc) = {
        let devices = ctx.devices()?;
        let mut found = None;
        for dev in devices.iter() {
            let d = dev.device_descriptor()?;
            if d.vendor_id() == VID && d.product_id() == PID {
                found = Some((dev.open()?, d));
                break;
            }
        }
        found.ok_or(anyhow!("POD Go not found. Is it connected?"))?
    };

    println!("Device opened");
    println!("  Bus: {}, Address: {}", handle.device().bus_number(), handle.device().address());
    println!("  USB version: {:04x}", desc.usb_version().0);

    // Enable auto-detach of kernel drivers (supported by libusb >=1.0.16)
    handle.set_auto_detach_kernel_driver(true)?;
    println!("Auto-detach enabled");

    // Claim vendor interface 0 (where HX protocol endpoints live)
    let iface = 0;
    match handle.claim_interface(iface) {
        Ok(_) => println!("Interface {} claimed", iface),
        Err(e) => {
            // Fallback: try explicit detach of kernel driver
            println!("Direct claim failed ({}), trying manual detach...", e);
            handle.detach_kernel_driver(iface)?;
            handle.claim_interface(iface)?;
            println!("Interface {} claimed after detach", iface);
        }
    }

    handle.clear_halt(EP_OUT)?;
    handle.clear_halt(EP_IN)?;
    println!("Endpoints cleared");

    drain(&handle)?;

    // Phase 0: Session handshake (5 packets)
    println!("=== Session Handshake ===");
    let mut buf = [0u8; 512];

    println!("Packet 1/5: HANDSHAKE");
    write_read(&handle, &handshake_packet(), &mut buf)?;

    println!("Packet 2/5: SESSION_OPEN_1");
    write_read(&handle, &session_open_1(), &mut buf)?;

    println!("Packet 3/5: SESSION_CHUNK_1");
    write_read(&handle, &session_chunk_1(), &mut buf)?;

    println!("Packet 4/5: SESSION_OPEN_2");
    write_read(&handle, &session_open_2(), &mut buf)?;

    println!("Packet 5/5: SESSION_CHUNK_2");
    write_read(&handle, &session_chunk_2(), &mut buf)?;

    println!("Session handshake complete");

    // Phase 1: Open preset resource
    println!("=== Preset Listing ===");
    println!("OPEN_PRESETS");
    write_read(&handle, &open_presets(), &mut buf)?;

    // Phase 2: Start paged stream
    println!("OPEN_STREAM (chunk #0)");
    let n = write_read(&handle, &open_stream(), &mut buf)?;
    let mut stream = Vec::new();
    if n > 16 {
        stream.extend_from_slice(&buf[16..n]);
    }
    println!("  Chunk #0: {} payload bytes", n.saturating_sub(16));

    // Phase 3: Chunk loop
    let mut seq: u8 = 0x08;
    let mut offset: u32 = 0x0000_1138;
    let mut chunk_num = 1;

    loop {
        let req = chunk_request(seq, offset);
        let n = write_read(&handle, &req, &mut buf)?;
        if n > 16 {
            stream.extend_from_slice(&buf[16..n]);
        }
        println!("  Chunk #{}: read {} bytes (payload {})", chunk_num, n, n.saturating_sub(16));
        if n < MAX_CHUNK {
            println!("  Short read → end of stream");
            break;
        }
        offset += 0x0100;
        seq = seq.wrapping_add(1);
        chunk_num += 1;
    }

    println!("Total stream size: {} bytes", stream.len());

    // Phase 4: Parse MessagePack (minimal decoder)
    println!("=== Parsing Presets ===");

    // Find DC 00 80 marker (MessagePack array16, 128 elements)
    let marker = [0xDC, 0x00, 0x80];
    let start = stream.windows(3).position(|w| w == marker)
        .ok_or(anyhow!("DC 00 80 marker not found in stream"))?;

    println!("Found DC 00 80 at offset {}", start);
    let mut pos = start;

    // Expect array16(128)
    assert_eq!(stream[pos], 0xDC, "Expected array16 marker");
    let count = u16::from_be_bytes([stream[pos+1], stream[pos+2]]) as usize;
    pos += 3;
    println!("Array of {} elements", count);

    for elem_idx in 0..count {
        if pos >= stream.len() {
            println!("  [{}] Truncated stream at pos {}", elem_idx, pos);
            break;
        }
        // Expect fixmap(1)
        if stream[pos] != 0x81 {
            println!("  [{}] Expected fixmap(1), got {:02x}", elem_idx, stream[pos]);
            break;
        }
        pos += 1;

        // Key: should be uint16 (0xCD) representing the preset index
        let (key, key_len) = decode_msgpack_int(&stream, pos);
        pos += key_len;
        let index = key as u16;

        // Value: should be a map containing preset fields
        let map_byte = stream[pos];
        let map_entries = if (map_byte & 0xF0) == 0x80 {
            // fixmap
            let n = (map_byte & 0x0F) as usize;
            pos += 1;
            n
        } else if map_byte == 0xDE {
            // map16
            let n = u16::from_be_bytes([stream[pos+1], stream[pos+2]]) as usize;
            pos += 3;
            n
        } else {
            println!("  [{}] Expected map at pos {}, got {:02x}", elem_idx, pos, map_byte);
            break;
        };

        let mut name = String::from("???");
        for _ in 0..map_entries {
            let (field_key, klen) = decode_msgpack_int(&stream, pos);
            pos += klen;

            if field_key == 109 {
                // str value: preset name
                let (val, vlen) = decode_msgpack_str(&stream, pos);
                pos += vlen;
                name = val.trim_end_matches('\0').to_string();
            } else {
                // skip unknown field value
                let slen = skip_msgpack_value(&stream, pos);
                pos += slen;
            }
        }

        println!("  [{:3}] {}", index, name);
    }

    handle.release_interface(iface)?;
    println!("Interface released. Done.");

    Ok(())
}

fn decode_msgpack_int(data: &[u8], pos: usize) -> (u64, usize) {
    if pos >= data.len() { return (0, 1); }
    let b = data[pos];
    if b <= 0x7F {
        (b as u64, 1)
    } else if b == 0xCC {
        if pos + 1 >= data.len() { return (0, 1); }
        (data[pos+1] as u64, 2)
    } else if b == 0xCD {
        if pos + 2 >= data.len() { return (0, 1); }
        (u16::from_be_bytes([data[pos+1], data[pos+2]]) as u64, 3)
    } else if b == 0xCE {
        if pos + 4 >= data.len() { return (0, 1); }
        (u32::from_be_bytes([data[pos+1], data[pos+2], data[pos+3], data[pos+4]]) as u64, 5)
    } else {
        (0, 1)
    }
}

fn decode_msgpack_str(data: &[u8], pos: usize) -> (String, usize) {
    if pos >= data.len() {
        return (String::new(), 1);
    }
    let b = data[pos];
    let (len, header_size) = if (b & 0xE0) == 0xA0 {
        // fixstr (0xA0-0xBF)
        ((b & 0x1F) as usize, 1)
    } else if b == 0xD9 {
        // str8
        if pos + 1 >= data.len() { return (String::new(), 1); }
        (data[pos+1] as usize, 2)
    } else if b == 0xDA {
        // str16
        if pos + 3 > data.len() { return (String::new(), 1); }
        (u16::from_be_bytes([data[pos+1], data[pos+2]]) as usize, 3)
    } else {
        return (format!("?{:02x}", b), 1);
    };
    let end = pos + header_size + len;
    let s = if end <= data.len() {
        String::from_utf8_lossy(&data[pos+header_size..end]).to_string()
    } else {
        String::from_utf8_lossy(&data[pos+header_size..]).to_string()
    };
    (s, header_size + len)
}

fn skip_msgpack_value(data: &[u8], pos: usize) -> usize {
    if pos >= data.len() { return 1; }
    let b = data[pos];
    match b {
        0xC0 => 1, // nil
        0xC2 | 0xC3 => 1, // bool false/true
        0xCC if pos + 1 < data.len() => 2, // uint8
        0xCD if pos + 2 < data.len() => 3, // uint16
        0xCE if pos + 4 < data.len() => 5, // uint32
        0xCA if pos + 4 < data.len() => 5, // float32
        0xCB if pos + 8 < data.len() => 9, // float64
        0xD9 if pos + 1 < data.len() => { let len = data[pos+1] as usize; 2 + len } // str8
        0xDA if pos + 2 < data.len() => { let len = u16::from_be_bytes([data[pos+1], data[pos+2]]) as usize; 3 + len } // str16
        0xDB if pos + 4 < data.len() => { let len = u32::from_be_bytes([data[pos+1], data[pos+2], data[pos+3], data[pos+4]]) as usize; 5 + len } // str32
        0xC4 if pos + 1 < data.len() => { let len = data[pos+1] as usize; 2 + len } // bin8
        0xC5 if pos + 2 < data.len() => { let len = u16::from_be_bytes([data[pos+1], data[pos+2]]) as usize; 3 + len } // bin16
        0xDC if pos + 2 < data.len() => { let n = u16::from_be_bytes([data[pos+1], data[pos+2]]) as usize; 3 + skip_n_values(data, pos+3, n) } // array16
        0xDE if pos + 2 < data.len() => { let n = u16::from_be_bytes([data[pos+1], data[pos+2]]) as usize; 3 + skip_n_map_entries(data, pos+3, n) } // map16
        _ => {
            if b <= 0x7F { 1 } // positive fixint
            else if (b & 0xE0) == 0xA0 { 1 + (b & 0x1F) as usize } // fixstr
            else if (b & 0xF0) == 0x80 { 1 + skip_n_map_entries(data, pos+1, (b & 0x0F) as usize) } // fixmap
            else if (b & 0xF0) == 0x90 { 1 + skip_n_values(data, pos+1, (b & 0x0F) as usize) } // fixarray
            else { 1 }
        }
    }
}

fn skip_n_values(data: &[u8], mut pos: usize, n: usize) -> usize {
    let start = pos;
    for _ in 0..n {
        let len = skip_msgpack_value(data, pos);
        pos += len;
    }
    pos - start
}

fn skip_n_map_entries(data: &[u8], mut pos: usize, n: usize) -> usize {
    let start = pos;
    for _ in 0..n {
        let klen = skip_msgpack_value(data, pos);
        pos += klen;
        let vlen = skip_msgpack_value(data, pos);
        pos += vlen;
    }
    pos - start
}
