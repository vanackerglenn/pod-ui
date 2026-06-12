use log::*;
use rusb::UsbContext;

const VID: u16 = 0x0e41;
const PID: u16 = 0x4247;
const EP_OUT: u8 = 0x01;
const EP_IN: u8 = 0x81;
const TIMEOUT: std::time::Duration = std::time::Duration::from_secs(2);
const VALIDATION_VEC: [u8; 16] = [
    0x08, 0x00, 0x00, 0x18, 0x01, 0x10, 0xAF, 0x03,
    0x00, 0x00, 0x00, 0x08, 0x1A, 0x10, 0x00, 0x00,
];

const HANDSHAKE: [u8; 20] = [
    0x0F, 0x00, 0x00, 0x18, 0x01, 0x10, 0xEF, 0x03,
    0x00, 0x01, 0x00, 0x08, 0x1A, 0x10, 0x00, 0x00,
    0x10, 0x00, 0x00, 0x00,
];

const SESSION_OPEN_1: [u8; 28] = [
    0x11, 0x00, 0x00, 0x18, 0x01, 0x10, 0xEF, 0x03,
    0x00, 0x02, 0x00, 0x0E, 0x1A, 0x10, 0x00, 0x00,
    0x00, 0x90, 0x09, 0x04, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00,
];

const SESSION_CHUNK_1: [u8; 16] = [
    0x08, 0x00, 0x00, 0x18, 0x01, 0x10, 0xEF, 0x03,
    0x00, 0x03, 0x00, 0x08, 0x1A, 0x10, 0x00, 0x00,
];

const SESSION_OPEN_2: [u8; 36] = [
    0x19, 0x00, 0x00, 0x18, 0x01, 0x10, 0xEF, 0x03,
    0x00, 0x04, 0x00, 0x10, 0x1A, 0x10, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00,
];

const SESSION_CHUNK_2: [u8; 16] = [
    0x08, 0x00, 0x00, 0x18, 0x01, 0x10, 0xEF, 0x03,
    0x00, 0x05, 0x00, 0x08, 0x1A, 0x10, 0x00, 0x00,
];

fn open_presets(seq: u16) -> [u8; 36] {
    let mut pkt = [
        0x19, 0x00, 0x00, 0x18, 0x01, 0x10, 0xEF, 0x03,
        0x00, 0x00, 0x00, 0x04, 0x1A, 0x10, 0x00, 0x00,
        0x01, 0x00, 0x02, 0x00, 0x09, 0x00, 0x00, 0x00,
        0x83, 0x66, 0xCD, 0x03, 0xE9, 0x64, 0x00, 0x65,
        0xC0, 0x00, 0x00, 0x00,
    ];
    pkt[9..11].copy_from_slice(&seq.to_le_bytes());
    pkt
}

fn open_stream(seq: u16) -> [u8; 40] {
    let mut pkt = [
        0x1D, 0x00, 0x00, 0x18, 0x01, 0x10, 0xEF, 0x03,
        0x00, 0x00, 0x00, 0x0C, 0x38, 0x10, 0x00, 0x00,
        0x01, 0x00, 0x02, 0x00, 0x0D, 0x00, 0x00, 0x00,
        0x83, 0x66, 0xCD, 0x03, 0xEA, 0x64, 0x01, 0x65,
        0x82, 0x6B, 0x00, 0x65, 0x02, 0x00, 0x00, 0x00,
    ];
    pkt[9..11].copy_from_slice(&seq.to_le_bytes());
    pkt
}



pub fn fetch_preset_names() -> anyhow::Result<Vec<(u16, String)>> {
    let ctx = rusb::Context::new()?;
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

    drain_stale(&handle)?;
    let seq = do_handshake(&handle)?;
    let data = list_presets(&handle, seq)?;

    handle.release_interface(iface)?;

    Ok(parse_preset_names(&data))
}

fn drain_stale(handle: &rusb::DeviceHandle<rusb::Context>) -> anyhow::Result<()> {
    let mut buf = [0u8; 512];
    loop {
        match handle.read_bulk(EP_IN, &mut buf, std::time::Duration::from_millis(50)) {
            Ok(n) => trace!("  Drain: read {} bytes", n),
            Err(rusb::Error::Timeout) => break,
            Err(e) => {
                warn!("  Drain error: {}", e);
                break;
            }
        }
    }
    Ok(())
}

fn do_handshake(handle: &rusb::DeviceHandle<rusb::Context>) -> anyhow::Result<u16> {
    let mut seq: u16 = 1;

    // Validate
    let mut buf = [0u8; 16];
    write_all(handle, EP_OUT, &VALIDATION_VEC)?;
    read_exact(handle, EP_IN, &mut buf)?;

    // Handshake
    write_all(handle, EP_OUT, &HANDSHAKE)?;
    read_exact(handle, EP_IN, &mut buf)?;
    seq += 1;

    // Session open 1
    write_all(handle, EP_OUT, &SESSION_OPEN_1)?;
    read_exact(handle, EP_IN, &mut buf)?;
    seq += 1;

    // Session chunk 1
    write_all(handle, EP_OUT, &SESSION_CHUNK_1)?;
    read_exact(handle, EP_IN, &mut buf)?;
    seq += 1;

    let session_open_2 = SESSION_OPEN_2;
    write_all(handle, EP_OUT, &session_open_2)?;
    read_exact(handle, EP_IN, &mut buf)?;
    seq += 1;

    write_all(handle, EP_OUT, &SESSION_CHUNK_2)?;
    read_exact(handle, EP_IN, &mut buf)?;
    seq += 1;

    Ok(seq)
}

fn list_presets(handle: &rusb::DeviceHandle<rusb::Context>, mut seq: u16) -> anyhow::Result<Vec<u8>> {
    let mut resp = [0u8; 512];

    // OPEN_PRESETS
    let pkt = open_presets(seq);
    write_all(handle, EP_OUT, &pkt)?;
    let n = read_into(handle, EP_IN, &mut resp)?;
    trace!("OPEN_PRESETS response: {} bytes", n);
    seq += 1;

    // OPEN_STREAM
    let pkt = open_stream(seq);
    write_all(handle, EP_OUT, &pkt)?;
    let n = read_into(handle, EP_IN, &mut resp)?;
    trace!("OPEN_STREAM response: {} bytes", n);
    seq += 1;

    // Collect stream chunks
    const CHUNK_SIZE: usize = 256;
    let mut stream = Vec::new();
    loop {
        // The stream requester: 16 bytes with seq and offset
        let mut req = [0u8; 16];
        req[0..4].copy_from_slice(&[0x08, 0x00, 0x00, 0x18]);
        req[4..8].copy_from_slice(&[0x01, 0x10, 0xEF, 0x03]);
        req[8..10].copy_from_slice(&seq.to_le_bytes());
        req[10..12].copy_from_slice(&[0x00, 0x08]);
        req[12..16].copy_from_slice(&[0x1A, 0x10, 0x00, 0x00]);

        write_all(handle, EP_OUT, &req)?;

        let n = read_into(handle, EP_IN, &mut resp)?;
        seq = seq.wrapping_add(1);

        // UsbPacket: 16 bytes header + payload
        if n > 16 {
            stream.extend_from_slice(&resp[16..n]);
        }
        trace!("  Chunk: read {} bytes (payload {})", n, n.saturating_sub(16));

        if n < CHUNK_SIZE + 16 {
            break;
        }
    }

    Ok(stream)
}

fn parse_preset_names(data: &[u8]) -> Vec<(u16, String)> {
    let marker = [0xDC, 0x00, 0x80];
    let start = data.windows(3).position(|w| w == marker).unwrap_or(0);
    let mut pos = start + 3;
    let mut names = Vec::with_capacity(128);

    for _ in 0..128 {
        if pos >= data.len() {
            break;
        }
        // Expect fixmap(1)
        if data[pos] != 0x81 {
            break;
        }
        pos += 1;

        // Key: preset index
        let (key, klen) = decode_msgpack_int(data, pos);
        pos += klen;
        let index = key as u16;

        // Value: map with fields
        let map_byte = data[pos];
        let map_entries = if (map_byte & 0xF0) == 0x80 {
            let n = (map_byte & 0x0F) as usize;
            pos += 1;
            n
        } else {
            break;
        };

        let mut name = String::from("???");
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
    let b = data[pos];
    if b <= 0x7F {
        (b as u64, 1)
    } else if b == 0xCC {
        (data[pos+1] as u64, 2)
    } else if b == 0xCD {
        (u16::from_be_bytes([data[pos+1], data[pos+2]]) as u64, 3)
    } else if b == 0xCE {
        (u32::from_be_bytes([data[pos+1], data[pos+2], data[pos+3], data[pos+4]]) as u64, 5)
    } else {
        (0, 0)
    }
}

fn decode_msgpack_str(data: &[u8], pos: usize) -> (String, usize) {
    let b = data[pos];
    let (len, hdr) = if (b & 0xE0) == 0xA0 {
        ((b & 0x1F) as usize, 1)
    } else if b == 0xD9 {
        (data[pos+1] as usize, 2)
    } else if b == 0xDA {
        (u16::from_be_bytes([data[pos+1], data[pos+2]]) as usize, 3)
    } else {
        return (String::new(), 0);
    };
    let s = String::from_utf8_lossy(&data[pos+hdr..pos+hdr+len]).to_string();
    (s, hdr + len)
}

fn skip_msgpack_value(data: &[u8], pos: usize) -> usize {
    let b = data[pos];
    match b {
        0xC0 | 0xC2 | 0xC3 => 1,
        0xCC => 2,
        0xCD => 3,
        0xCE => 5,
        0xD9 => 2 + data[pos+1] as usize,
        0xDA => 3 + u16::from_be_bytes([data[pos+1], data[pos+2]]) as usize,
        0xDB => 5 + u32::from_be_bytes([data[pos+1], data[pos+2], data[pos+3], data[pos+4]]) as usize,
        _ if b <= 0x7F => 1,
        _ if (b & 0xE0) == 0xA0 => 1 + (b & 0x1F) as usize,
        _ if (b & 0xF0) == 0x80 => 1 + skip_n_map_entries(data, pos+1, (b & 0x0F) as usize),
        _ => 0,
    }
}

fn skip_n_map_entries(data: &[u8], mut pos: usize, n: usize) -> usize {
    let start = pos;
    for _ in 0..n {
        pos += skip_msgpack_value(data, pos);
        pos += skip_msgpack_value(data, pos);
    }
    pos - start
}

fn write_all(handle: &rusb::DeviceHandle<rusb::Context>, ep: u8, data: &[u8]) -> anyhow::Result<()> {
    let written = handle.write_bulk(ep, data, TIMEOUT)?;
    if written != data.len() {
        anyhow::bail!("Short write: {} != {}", written, data.len());
    }
    Ok(())
}

fn read_exact(handle: &rusb::DeviceHandle<rusb::Context>, ep: u8, buf: &mut [u8]) -> anyhow::Result<()> {
    let n = handle.read_bulk(ep, buf, TIMEOUT)?;
    if n != buf.len() {
        anyhow::bail!("Short read: {} != {}", n, buf.len());
    }
    Ok(())
}

fn read_into(handle: &rusb::DeviceHandle<rusb::Context>, ep: u8, buf: &mut [u8]) -> anyhow::Result<usize> {
    Ok(handle.read_bulk(ep, buf, TIMEOUT)?)
}
