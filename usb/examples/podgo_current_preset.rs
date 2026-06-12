// Full preset data reader: establish all channels, request preset data, parse it
use std::time::Duration;
use anyhow::{anyhow, Result};
use rusb::{Context, UsbContext};

const VID: u16 = 0x0e41;
const PID: u16 = 0x4247;
const EP_OUT: u8 = 0x01;
const EP_IN: u8 = 0x81;

fn drain(handle: &rusb::DeviceHandle<Context>) {
    let mut buf = [0u8; 512];
    loop { match handle.read_bulk(EP_IN, &mut buf, Duration::from_millis(30)) { Ok(_) => {}, Err(_) => break } }
}

fn xfer(handle: &rusb::DeviceHandle<Context>, data: &[u8], label: &str, ms: u64) -> Result<Vec<u8>> {
    let mut buf = [0u8; 4096];
    handle.write_bulk(EP_OUT, data, Duration::from_millis(ms))?;
    let n = handle.read_bulk(EP_IN, &mut buf, Duration::from_millis(ms))?;
    Ok(buf[..n].to_vec())
}

fn fetch_preset_data(handle: &rusb::DeviceHandle<Context>) -> Result<Vec<u8>> {
    // Phase 1: x1 session (sub_type=5)
    xfer(handle, &[0x0C,0,0,0x28,1,0x10,0xEF,3,0,0,0,2,0,1,0,0x21,0,0x10,0,0], "x1 HANDSHAKE", 3000)?;
    xfer(handle, &[0x11,0,0,0x18,1,0x10,0xEF,3,0,2,0,4,0,0x10,0,0,1,0,5,0,1,0,0,0,5,0,0,0], "x1 SESS_OPEN", 3000)?;
    xfer(handle, &[0x08,0,0,0x18,1,0x10,0xEF,3,0,3,0,8,0x20,0x10,0,0], "x1 CHUNK_READ", 3000)?;
    xfer(handle, &[0x08,0,0,0x18,1,0x10,0xEF,3,0,4,0,2,0x20,0x10,0,0], "x1 CMD_0002", 3000)?;

    // Phase 2: x80 channel
    xfer(handle, &[0x0C,0,0,0x28,0x80,0x10,0xED,3,0,0,0,2,0,1,0,0x21,0,0x10,0,0], "x80 HANDSHAKE", 3000)?;
    xfer(handle, &[0x11,0,0,0x18,0x80,0x10,0xED,3,0,2,0,4,0,0x10,0,0,1,0,6,0,1,0,0,0,6,0,0,0], "x80 SESS_OPEN", 3000)?;

    // Phase 3: x2 channel
    xfer(handle, &[0x0C,0,0,0x28,2,0x10,0xF0,3,0,0,0,2,0,1,0,0x21,0,0x10,0,0], "x2 HANDSHAKE", 3000)?;
    xfer(handle, &[0x11,0,0,0x18,2,0x10,0xF0,3,0,2,0,4,0,0x10,0,0,1,0,4,0,1,0,0,0,4,0,0,0], "x2 SESS_OPEN", 3000)?;

    // Phase 4: Open resource 1000 on x80
    xfer(handle, &[
        0x19,0,0,0x18,0x80,0x10,0xED,3,0,3,0,4,
        0x09,0x10,0,0, 1,0,6,0,9,0,0,0,
        0x83,0x66,0xCD,3,0xE8,0x64,0x4C,0x65,0x80,0,0,0
    ], "x80 OPEN_1000", 3000)?;

    // Phase 5: Request preset data (first chunk comes back as response)
    let mut all_data = vec![];
    let mut seq: u8 = 4;

    let r = xfer(handle, &[
        0x19,0,0,0x18,0x80,0x10,0xED,3,0,seq,0,0x0C,
        0x0F, 0x10, 0x00, 0,
        1,0,6,0,9,0,0,0,
        0x83,0x66,0xCD,3,0xF4,0x64,0x16,0x65,0xC0,0,0,0
    ], "x80 REQ_PRESET", 3000)?;

    if r.len() > 16 {
        all_data.extend_from_slice(&r[16..]);
        eprintln!("  chunk[0]: +{} payload (from request)", r.len() - 16);
    }

    // Phase 6: Read remaining chunks via keep-alive (device streams data in response to any x80 write)
    for chunk in 1..60 {
        seq = seq.wrapping_add(1);
        // Send x80 keep-alive/dummy, device responds with next data chunk
        let mut buf = [0u8; 4096];
        let _ = handle.write_bulk(EP_OUT, &[
            0x08,0,0,0x18,0x80,0x10,0xED,3,0,seq,0,8,
            0x0F, 0x10, 0x00, 0
        ], Duration::from_millis(200));

        match handle.read_bulk(EP_IN, &mut buf, Duration::from_millis(2000)) {
            Ok(n) => {
                if n <= 16 {
                    eprintln!("  chunk[{chunk}]: end marker ({n}b) — preset data complete");
                    eprintln!("  Total: {} bytes in {} chunks", all_data.len(), chunk);
                    break;
                }
                let payload = &buf[16..n];
                all_data.extend_from_slice(payload);
                if chunk < 4 || chunk % 10 == 0 {
                    eprintln!("  chunk[{chunk}]: +{}b payload (total {})", payload.len(), all_data.len());
                }
            }
            Err(rusb::Error::Timeout) => {
                eprintln!("  chunk[{chunk}]: timeout — no more data. Total: {} bytes", all_data.len());
                break;
            }
            Err(e) => anyhow::bail!("chunk {chunk}: {e}"),
        }
    }
    Ok(all_data)
}

// ============= Preset Parser =============

fn hex_str(data: &[u8]) -> String {
    data.iter().map(|b| format!("{b:02x}")).collect()
}

fn find_marker(data: &[u8], marker: &[u8]) -> Option<usize> {
    data.windows(marker.len()).position(|w| w == marker)
}

fn parse_fixstr(data: &[u8]) -> Option<(String, usize)> {
    // MessagePack fixstr: 0xa0-0xbf, length = byte & 0x1f, followed by string bytes
    if data.is_empty() { return None; }
    let b = data[0];
    if b >= 0xa0 && b <= 0xbf {
        let len = (b & 0x1f) as usize;
        if data.len() >= 1 + len {
            // Strip null padding
            let s = String::from_utf8_lossy(&data[1..1+len]).trim_end_matches('\0').to_string();
            return Some((s, 1 + len));
        }
    }
    // Also handle raw strings: if byte is 0x05 (fixint 5 = key?), then next byte is fixstr...
    // Actually looking at the data, the pattern is: key_byte(05) fixstr_byte(str_len) name_bytes
    // But we can also handle embedded strings directly by checking for fixstr pattern
    None
}

fn parse_fixint(data: &[u8]) -> Option<(i64, usize)> {
    if data.is_empty() { return None; }
    let b = data[0];
    if b <= 0x7f {
        return Some((b as i64, 1));
    }
    if b >= 0xe0 {
        return Some((b as i8 as i64, 1));
    }
    match b {
        0xcc => { if data.len() >= 2 { Some((data[1] as i64, 2)) } else { None } }
        0xcd => { if data.len() >= 3 { let v = (data[1] as u16) << 8 | data[2] as u16; Some((v as i64, 3)) } else { None } }
        0xce => { if data.len() >= 5 { let v = (data[1] as u32) << 24 | (data[2] as u32) << 16 | (data[3] as u32) << 8 | data[4] as u32; Some((v as i64, 5)) } else { None } }
        0xd0 => { if data.len() >= 2 { Some((data[1] as i8 as i64, 2)) } else { None } }
        0xd1 => { if data.len() >= 3 { let v = (data[1] as i16 as i64) << 8 | data[2] as i16 as i64; Some((v, 3)) } else { None } }
        0xd2 => { if data.len() >= 5 { let v = (data[1] as i32 as i64) << 24 | (data[2] as i32 as i64) << 16 | (data[3] as i32 as i64) << 8 | data[4] as i32 as i64; Some((v, 5)) } else { None } }
        _ => None,
    }
}

fn parse_float32(data: &[u8]) -> Option<(f32, usize)> {
    if data.len() >= 5 && data[0] == 0xca {
        let bits = ((data[1] as u32) << 24) | ((data[2] as u32) << 16) | ((data[3] as u32) << 8) | data[4] as u32;
        Some((f32::from_bits(bits), 5))
    } else {
        None
    }
}

fn parse_bool(data: &[u8]) -> Option<(bool, usize)> {
    if data.is_empty() { return None; }
    match data[0] {
        0xc2 => Some((false, 1)),
        0xc3 => Some((true, 1)),
        _ => None,
    }
}

fn parse_nil(data: &[u8]) -> Option<usize> {
    if !data.is_empty() && data[0] == 0xc0 { Some(1) } else { None }
}

#[derive(Debug)]
struct Param {
    id: u8,
    value: f32,
    raw: Vec<u8>,
}

#[derive(Debug)]
struct Module {
    name: String,
    slot: u8,
    bypassed: bool,
    params: Vec<Param>,
}

#[derive(Debug)]
struct Snapshot {
    name: String,
    index: u8,
}

fn parse_modules(data: &[u8]) -> (Vec<Module>, Vec<Snapshot>) {
    let hex = hex_str(data);
    let mut modules = vec![];
    let mut snapshots = vec![];
    let mut pos = 0;

    // Find all module entries: 91 87 0a 00 0b 85 00 01
    // The module marker is `91 87`
    loop {
        match find_marker(&data[pos..], &[0x91, 0x87]) {
            Some(off) => {
                let start = pos + off;
                let rest = &data[start..];

                // After marker: 0a 00 0b 85 00 01 05 (key=5)
                let mut offset = 2; // skip 91 87

                // Skip ahead to find the name (fixstr after key 05)
                // Pattern: ... 05 [fixstr] name ...
                let mut name_pos = offset;
                while name_pos < rest.len() {
                    if name_pos + 1 < rest.len() && rest[name_pos] == 0x05 {
                        let name_byte = rest[name_pos + 1];
                        if name_byte >= 0xa0 && name_byte <= 0xbf {
                            // Found name!
                            if let Some((n, nlen)) = parse_fixstr(&rest[name_pos + 1..]) {
                                // Extract slot index from before the name
                                // Look backwards for 0x08 byte before 91 87
                                let slot = if start >= 2 { data[start - 2] } else { 0 };

                                modules.push(Module {
                                    name: n.clone(),
                                    slot,
                                    bypassed: false,
                                    params: vec![],
                                });

                                // Check if this name looks like a snapshot
                                if n.starts_with("SNAPSHOT") {
                                    let idx = n.trim_start_matches("SNAPSHOT ").parse().unwrap_or(0);
                                    snapshots.push(Snapshot { name: n, index: idx });
                                }
                                break;
                            }
                        }
                    }
                    name_pos += 1;
                    if name_pos > 100 { break; }
                }
                pos = start + 1; // continue from after 91 87
            }
            None => break,
        }
    }

    (modules, snapshots)
}

// Alternative simpler parser: find all NUL-terminated strings of reasonable length
fn extract_strings(data: &[u8]) -> Vec<String> {
    let mut strings = vec![];
    let mut i = 0;
    while i < data.len() {
        if data[i] >= 0x20 && data[i] < 0x7f {
            let start = i;
            while i < data.len() && data[i] >= 0x20 && data[i] < 0x7f { i += 1; }
            let s: String = data[start..i].iter().map(|&c| c as char).collect();
            if s.len() >= 3 && !s.starts_with("ca") && !s.starts_with("ce")
                && !s.starts_with("cd") && !s.starts_with("d0") && !s.starts_with("da")
            {
                strings.push(s);
            }
        } else {
            i += 1;
        }
    }
    strings
}

fn main() -> Result<()> {
    let ctx = Context::new()?;
    let devices = ctx.devices()?;
    let mut dev = None;
    for d in devices.iter() {
        let desc = d.device_descriptor()?;
        if desc.vendor_id() == VID && desc.product_id() == PID { dev = Some(d); break; }
    }
    let dev = dev.ok_or(anyhow!("Pod Go not found"))?;
    let h = dev.open()?;
    h.set_auto_detach_kernel_driver(true)?;
    h.claim_interface(0)?;
    h.clear_halt(EP_OUT)?;
    h.clear_halt(EP_IN)?;
    drain(&h);

    println!("=== Fetching preset data ===");
    let preset_data = fetch_preset_data(&h)?;
    h.release_interface(0)?;

    println!("\n=== Parsing Preset ===");

    // Parse modules
    let (modules, snapshots) = parse_modules(&preset_data);
    println!("\nFound {} modules:", modules.len());
    for (i, m) in modules.iter().enumerate() {
        println!("  {}. {} (slot=0x{:02x})", i+1, m.name, m.slot);
    }

    println!("\nSnapshots: {:?}", snapshots.iter().map(|s| &s.name).collect::<Vec<_>>());

    // Extract all meaningful strings
    println!("\n=== All Strings Found ===");
    let strings = extract_strings(&preset_data);
    for s in &strings {
        println!("  \"{}\"", s);
    }

    // Save raw data to file for offline analysis
    std::fs::write("/tmp/podgo_preset.bin", &preset_data)?;
    println!("\nRaw data saved to /tmp/podgo_preset.bin ({} bytes)", preset_data.len());

    // Also output hex dump in a format useful for analysis
    println!("\n=== Hex Dump (first 2KB) ===");
    for (i, chunk) in preset_data.chunks(32).enumerate() {
        if i > 63 { println!("  ..."); break; }
        let hex: Vec<String> = chunk.iter().map(|b| format!("{b:02x}")).collect();
        let asc: String = chunk.iter().map(|&b| if b >= 0x20 && b < 0x7f { b as char } else { '.' }).collect();
        println!("  {:04x}: {:96} {}", i*32, hex.join(" "), asc);
    }

    println!("\nDone.");
    Ok(())
}
