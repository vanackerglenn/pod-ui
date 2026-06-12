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

// Packet header helpers
fn packet_header(len: u8, seq: u16, cmd: u16) -> [u8; 12] {
    [len, 0x00, 0x00, 0x18, 0x01, 0x10, 0xEF, 0x03,
     (seq >> 8) as u8, seq as u8, (cmd >> 8) as u8, cmd as u8]
}

fn open_resource_payload(resource_id: u64, param: u8, extra: &[u8]) -> Vec<u8> {
    // Build a resource-open packet similar to open_presets
    // Payload format: {102: resource_id, 100: param, 101: extra...}
    let mut payload = vec![0x83, 0x66]; // fixmap(3), key=102
    // Value for 102: uint16(resource_id)
    payload.extend_from_slice(&[0xCD, (resource_id >> 8) as u8, resource_id as u8]);
    // Key=100, value=fixint(param)
    payload.extend_from_slice(&[0x64, param]);
    // Key=101 (extra options)
    payload.push(0x65);
    if extra.is_empty() {
        payload.push(0xC0); // nil
    } else {
        payload.extend_from_slice(extra);
    }
    payload
}

fn build_open_packet(resource_id: u64, param: u8, extra: &[u8], seq: u16) -> Vec<u8> {
    let payload = open_resource_payload(resource_id, param, extra);
    let total_len = 24 + payload.len(); // 12 header + 4 id + 8 block + payload
    let mut pkt = Vec::with_capacity(total_len);
    pkt.extend_from_slice(&packet_header(total_len as u8, seq, 0x0004)); // cmd=4 = open
    pkt.extend_from_slice(&[0x1A, 0x10, 0x00, 0x00]); // object id?
    pkt.extend_from_slice(&[0x01, 0x00, 0x02, 0x00]); // block hdr
    let plen = payload.len() as u32;
    pkt.extend_from_slice(&plen.to_le_bytes()); // payload length
    pkt.extend_from_slice(&payload);
    // Pad to 4-byte alignment if needed
    while pkt.len() % 4 != 0 { pkt.push(0x00); }
    pkt
}

fn build_open_stream_packet(seq: u16) -> Vec<u8> {
    // Replicate open_stream packet
    let payload: [u8; 13] = [0x83, 0x66, 0xCD, 0x03, 0xEA, 0x64, 0x01, 0x65, 0x82, 0x6B, 0x00, 0x65, 0x02];
    let mut pkt = Vec::new();
    pkt.extend_from_slice(&[0x1D, 0x00, 0x00, 0x18, 0x01, 0x10, 0xEF, 0x03,
        (seq >> 8) as u8, seq as u8, 0x00, 0x0C]);
    pkt.extend_from_slice(&[0x38, 0x10, 0x00, 0x00]); // offset
    pkt.extend_from_slice(&[0x01, 0x00, 0x02, 0x00]); // block hdr
    let plen = 13u32;
    pkt.extend_from_slice(&plen.to_le_bytes());
    pkt.extend_from_slice(&payload);
    pkt
}

fn chunk_request(seq: u8, offset: u32) -> [u8; 16] {
    let off = offset.to_le_bytes();
    [0x08,0x00,0x00,0x18,0x01,0x10,0xEF,0x03,0x00,seq,0x00,0x08,off[0],off[1],off[2],off[3]]
}

fn write_read(handle: &rusb::DeviceHandle<Context>, data: &[u8], buf: &mut [u8]) -> Result<usize> {
    let _ = handle.write_bulk(EP_OUT, data, TIMEOUT)?;
    Ok(handle.read_bulk(EP_IN, buf, TIMEOUT)?)
}

fn drain(handle: &rusb::DeviceHandle<Context>) -> Result<()> {
    let mut buf = [0u8; 512];
    loop { match handle.read_bulk(EP_IN, &mut buf, DRAIN_TIMEOUT) { Ok(_) => {}, Err(rusb::Error::Timeout) => break, Err(e) => bail!("Drain error: {}", e) } }
    Ok(())
}

fn do_handshake(handle: &rusb::DeviceHandle<Context>) -> Result<()> {
    let mut buf = [0u8; 512];
    // Standard 5-packet handshake from probe
    let handshake: [u8; 20] = [0x0C,0x00,0x00,0x28,0x01,0x10,0xEF,0x03,0x00,0x00,0x00,0x02,0x00,0x01,0x00,0x21,0x00,0x10,0x00,0x00];
    let s1: [u8; 28] = [0x11,0x00,0x00,0x18,0x01,0x10,0xEF,0x03,0x00,0x02,0x00,0x04,0x00,0x10,0x00,0x00,0x01,0x00,0x02,0x00,0x01,0x00,0x00,0x00,0x02,0x00,0x00,0x00];
    let sc1: [u8; 16] = [0x08,0x00,0x00,0x18,0x01,0x10,0xEF,0x03,0x00,0x03,0x00,0x08,0x09,0x10,0x00,0x00];
    let s2: [u8; 36] = [0x1A,0x00,0x00,0x18,0x01,0x10,0xEF,0x03,0x00,0x04,0x00,0x04,0x09,0x10,0x00,0x00,0x01,0x00,0x02,0x00,0x0A,0x00,0x00,0x00,0x83,0x66,0xCD,0x03,0xE8,0x64,0xCC,0xFE,0x65,0x80,0x00,0x00];
    let sc2: [u8; 16] = [0x08,0x00,0x00,0x18,0x01,0x10,0xEF,0x03,0x00,0x05,0x00,0x08,0x1A,0x10,0x00,0x00];
    write_read(handle, &handshake, &mut buf)?;
    write_read(handle, &s1, &mut buf)?;
    write_read(handle, &sc1, &mut buf)?;
    write_read(handle, &s2, &mut buf)?;
    write_read(handle, &sc2, &mut buf)?;
    Ok(())
}

fn read_stream(handle: &rusb::DeviceHandle<Context>) -> Result<Vec<u8>> {
    let mut buf = [0u8; 512];
    let open_presets: [u8; 36] = [0x19,0x00,0x00,0x18,0x01,0x10,0xEF,0x03,0x00,0x06,0x00,0x04,0x1A,0x10,0x00,0x00,0x01,0x00,0x02,0x00,0x09,0x00,0x00,0x00,0x83,0x66,0xCD,0x03,0xE9,0x64,0x00,0x65,0xC0,0x00,0x00,0x00];
    let stream_pkt = build_open_stream_packet(7);
    write_read(handle, &open_presets, &mut buf)?;

    let n = write_read(handle, &stream_pkt, &mut buf)?;
    let mut stream = Vec::new();
    if n > 16 { stream.extend_from_slice(&buf[16..n]); }

    let mut seq: u8 = 0x08;
    let mut offset: u32 = 0x0000_1138;
    loop {
        let req = chunk_request(seq, offset);
        let n = write_read(handle, &req, &mut buf)?;
        if n > 16 { stream.extend_from_slice(&buf[16..n]); }
        if n < MAX_CHUNK { break; }
        offset += 0x0100;
        seq = seq.wrapping_add(1);
    }
    Ok(stream)
}

fn try_open_resource(handle: &rusb::DeviceHandle<Context>, resource_id: u64, param: u8, extra: &[u8], seq: u16) -> Result<Vec<u8>> {
    let mut buf = [0u8; 512];
    let pkt = build_open_packet(resource_id, param, extra, seq);
    let n = write_read(handle, &pkt, &mut buf)?;
    if n > 16 {
        Ok(buf[16..n].to_vec())
    } else {
        Ok(Vec::new())
    }
}

fn main() -> Result<()> {
    let ctx = Context::new()?;
    let handle = {
        let devices = ctx.devices()?;
        let mut found = None;
        for dev in devices.iter() {
            let d = dev.device_descriptor()?;
            if d.vendor_id() == VID && d.product_id() == PID { found = Some(dev.open()?); break; }
        }
        found.ok_or(anyhow!("POD Go not found"))?
    };
    handle.set_auto_detach_kernel_driver(true)?;
    handle.claim_interface(0)?;
    handle.clear_halt(EP_OUT)?;
    handle.clear_halt(EP_IN)?;
    drain(&handle)?;

    // First: full handshake
    do_handshake(&handle)?;
    println!("=== Handshake done ===");

    // Try different resource IDs (1003-1010)
    // Also try 1001 with specific preset index as param
    let attempts: Vec<(u64, u8, &[u8], &str)> = vec![
        (1003, 0, &[], "resource 1003"),
        (1004, 0, &[], "resource 1004"),
        (1005, 0, &[], "resource 1005"),
        (1003, 1, &[], "resource 1003, param=1"),
        (1001, 0, &[0x82, 0x6B, 0x00, 0x65, 0x02], "1001 with extra stream opts"),
        (1001, 0, &[0x81, 0x6B, 0x00], "1001 with {107:0}"),
        (1001, 0, &[0x81, 0x6B, 0x7F], "1001 with {107:127}"),
        (1000, 0, &[], "resource 1000"),
    ];

    for (i, (rid, param, extra, desc)) in attempts.iter().enumerate() {
        let seq = 0x10 + i as u16;
        match try_open_resource(&handle, *rid, *param, extra, seq) {
            Ok(response) => {
                if response.is_empty() {
                    println!("{}: empty response", desc);
                } else {
                    println!("{}: {} bytes - hex preview:", desc, response.len());
                    for chunk in response[..response.len().min(64)].chunks(16) {
                        let hex: Vec<String> = chunk.iter().map(|b| format!("{:02x}", b)).collect();
                        println!("    {}", hex.join(" "));
                    }
                }
            }
            Err(e) => {
                println!("{}: error: {}", desc, e);
            }
        }
    }

    println!("\n=== Now trying full protocol with resource 1003 instead of 1001 ===");
    // Re-do handshake for clean state
    drain(&handle)?;
    do_handshake(&handle)?;
    
    // Try opening resource 1003 and then reading stream from it
    let mut buf = [0u8; 512];
    // Manually craft open for 1003 with param=0, extra=nil
    let open_1003 = build_open_packet(1003, 0, &[], 6);
    println!("Open 1003 packet ({} bytes):", open_1003.len());
    for chunk in open_1003.chunks(16) {
        let hex: Vec<String> = chunk.iter().map(|b| format!("{:02x}", b)).collect();
        println!("  {}", hex.join(" "));
    }
    let n = write_read(&handle, &open_1003, &mut buf)?;
    println!("Open 1003 response: {} bytes", n);
    if n > 12 {
        for chunk in buf[12..n].chunks(16) {
            let hex: Vec<String> = chunk[..16.min(chunk.len())].iter().map(|b| format!("{:02x}", b)).collect();
            println!("  {}", hex.join(" "));
        }
    }

    // Now try to read stream
    let stream_pkt = build_open_stream_packet(7);
    let n = write_read(&handle, &stream_pkt, &mut buf)?;
    println!("Stream response for 1003: {} bytes", n);
    if n > 16 {
        for chunk in buf[16..n.min(80)].chunks(16) {
            let hex: Vec<String> = chunk.iter().map(|b| format!("{:02x}", b)).collect();
            println!("  {}", hex.join(" "));
        }
    }

    handle.release_interface(0)?;
    println!("\nDone.");
    Ok(())
}
