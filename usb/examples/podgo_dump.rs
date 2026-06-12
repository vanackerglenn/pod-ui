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

fn handshake_packet() -> [u8; 20] { [0x0C,0x00,0x00,0x28,0x01,0x10,0xEF,0x03,0x00,0x00,0x00,0x02,0x00,0x01,0x00,0x21,0x00,0x10,0x00,0x00] }
fn session_open_1() -> [u8; 28] { [0x11,0x00,0x00,0x18,0x01,0x10,0xEF,0x03,0x00,0x02,0x00,0x04,0x00,0x10,0x00,0x00,0x01,0x00,0x02,0x00,0x01,0x00,0x00,0x00,0x02,0x00,0x00,0x00] }
fn session_chunk_1() -> [u8; 16] { [0x08,0x00,0x00,0x18,0x01,0x10,0xEF,0x03,0x00,0x03,0x00,0x08,0x09,0x10,0x00,0x00] }
fn session_open_2() -> [u8; 36] { [0x1A,0x00,0x00,0x18,0x01,0x10,0xEF,0x03,0x00,0x04,0x00,0x04,0x09,0x10,0x00,0x00,0x01,0x00,0x02,0x00,0x0A,0x00,0x00,0x00,0x83,0x66,0xCD,0x03,0xE8,0x64,0xCC,0xFE,0x65,0x80,0x00,0x00] }
fn session_chunk_2() -> [u8; 16] { [0x08,0x00,0x00,0x18,0x01,0x10,0xEF,0x03,0x00,0x05,0x00,0x08,0x1A,0x10,0x00,0x00] }
fn open_presets() -> [u8; 36] { [0x19,0x00,0x00,0x18,0x01,0x10,0xEF,0x03,0x00,0x06,0x00,0x04,0x1A,0x10,0x00,0x00,0x01,0x00,0x02,0x00,0x09,0x00,0x00,0x00,0x83,0x66,0xCD,0x03,0xE9,0x64,0x00,0x65,0xC0,0x00,0x00,0x00] }
fn open_stream() -> [u8; 40] { [0x1D,0x00,0x00,0x18,0x01,0x10,0xEF,0x03,0x00,0x07,0x00,0x0C,0x38,0x10,0x00,0x00,0x01,0x00,0x02,0x00,0x0D,0x00,0x00,0x00,0x83,0x66,0xCD,0x03,0xEA,0x64,0x01,0x65,0x82,0x6B,0x00,0x65,0x02,0x00,0x00,0x00] }
fn chunk_request(seq: u8, offset: u32) -> [u8; 16] { let off = offset.to_le_bytes(); [0x08,0x00,0x00,0x18,0x01,0x10,0xEF,0x03,0x00,seq,0x00,0x08,off[0],off[1],off[2],off[3]] }

fn write_read(handle: &rusb::DeviceHandle<Context>, data: &[u8], buf: &mut [u8]) -> Result<usize> { let _ = handle.write_bulk(EP_OUT, data, TIMEOUT)?; Ok(handle.read_bulk(EP_IN, buf, TIMEOUT)?) }
fn drain(handle: &rusb::DeviceHandle<Context>) -> Result<()> { let mut buf = [0u8; 512]; loop { match handle.read_bulk(EP_IN, &mut buf, DRAIN_TIMEOUT) { Ok(n) => eprintln!("  drain: read {} bytes", n), Err(rusb::Error::Timeout) => break, Err(e) => bail!("Drain error: {}", e) } } Ok(()) }

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

    let mut buf = [0u8; 512];
    write_read(&handle, &handshake_packet(), &mut buf)?;
    write_read(&handle, &session_open_1(), &mut buf)?;
    write_read(&handle, &session_chunk_1(), &mut buf)?;
    write_read(&handle, &session_open_2(), &mut buf)?;
    write_read(&handle, &session_chunk_2(), &mut buf)?;
    write_read(&handle, &open_presets(), &mut buf)?;

    let n = write_read(&handle, &open_stream(), &mut buf)?;
    let mut stream = Vec::new();
    if n > 16 { stream.extend_from_slice(&buf[16..n]); }

    let mut seq: u8 = 0x08;
    let mut offset: u32 = 0x0000_1138;
    loop {
        let req = chunk_request(seq, offset);
        let n = write_read(&handle, &req, &mut buf)?;
        if n > 16 { stream.extend_from_slice(&buf[16..n]); }
        if n < MAX_CHUNK { break; }
        offset += 0x0100;
        seq = seq.wrapping_add(1);
    }

    handle.release_interface(0)?;
    eprintln!("\nTotal stream: {} bytes\n", stream.len());

    // Dump hex preview of first 64 bytes
    println!("=== First 64 bytes of stream ===");
    for chunk in stream[..64.min(stream.len())].chunks(16) {
        let hex: Vec<String> = chunk.iter().map(|b| format!("{:02x}", b)).collect();
        println!("  {}", hex.join(" "));
    }

    // Find the DC 00 80 marker and show context
    let marker = [0xDC, 0x00, 0x80];
    if let Some(marker_pos) = stream.windows(3).position(|w| w == marker) {
        println!("\n=== DC 00 80 marker at offset {} ===", marker_pos);

        // Show bytes before marker
        let before = marker_pos.max(16) - 16;
        println!("  Bytes before marker ({}..{}):", before, marker_pos);
        for chunk in stream[before..marker_pos].chunks(16) {
            let hex: Vec<String> = chunk.iter().map(|b| format!("{:02x}", b)).collect();
            println!("    {}", hex.join(" "));
        }

        // Show marker and bytes after
        let after = (marker_pos + 256).min(stream.len());
        println!("  Bytes {}..{}:", marker_pos, after);
        for chunk in stream[marker_pos..after].chunks(16) {
            let hex: Vec<String> = chunk.iter().map(|b| format!("{:02x}", b)).collect();
            println!("    {}", hex.join(" "));
        }

        // Now parse all top-level values from the stream
        println!("\n=== Parsed all top-level values ===");
        let mut pos = 0;
        while pos < stream.len() {
            let b = stream[pos];
            if b == 0xDC {
                // array16 - this is the preset array
                let n = u16::from_be_bytes([stream[pos+1], stream[pos+2]]) as usize;
                println!("  Top-level: array16({}) at offset {}", n, pos);
                // Inside, each element is fixmap(1) with preset index -> preset data
                // Just skip this
                pos += 3;
                for _ in 0..n {
                    if pos >= stream.len() { break; }
                    assert_eq!(stream[pos] & 0xF0, 0x80, "Expected fixmap at {}", pos);
                    let entries = (stream[pos] & 0x0F) as usize;
                    pos += 1;
                    for _ in 0..entries {
                        let (_, kl) = decode_int(&stream, pos); pos += kl;
                        skip_value(&stream, &mut pos);
                    }
                }
            } else if b <= 0x7F {
                println!("  Top-level: fixint({}) at offset {}", b, pos);
                pos += 1;
            } else {
                println!("  Top-level: other({:02x}) at offset {}", b, pos);
                skip_value(&stream, &mut pos);
            }
        }
    }

    Ok(())
}

fn decode_int(data: &[u8], pos: usize) -> (u64, usize) {
    let b = data[pos];
    if b <= 0x7F { (b as u64, 1) }
    else if b == 0xCC { (data[pos+1] as u64, 2) }
    else if b == 0xCD { (u16::from_be_bytes([data[pos+1], data[pos+2]]) as u64, 3) }
    else { panic!("Unexpected int marker {:02x}", b) }
}

fn skip_value(data: &[u8], pos: &mut usize) {
    let b = data[*pos];
    match b {
        0xC0 | 0xC2 | 0xC3 => *pos += 1,
        0xCC => *pos += 2, 0xCD => *pos += 3, 0xCE => *pos += 5,
        0xD9 => { *pos += 2 + data[*pos+1] as usize; }
        0xDA => { *pos += 3 + u16::from_be_bytes([data[*pos+1], data[*pos+2]]) as usize; }
        0xC4 => { *pos += 2 + data[*pos+1] as usize; }
        0xC5 => { *pos += 3 + u16::from_be_bytes([data[*pos+1], data[*pos+2]]) as usize; }
        _ if (b & 0xE0) == 0xA0 => *pos += 1 + (b & 0x1F) as usize,
        _ if (b & 0xF0) == 0x80 => { let n = (b & 0x0F) as usize; *pos += 1; for _ in 0..n { skip_value(data, pos); skip_value(data, pos); } }
        _ if (b & 0xF0) == 0x90 => { let n = (b & 0x0F) as usize; *pos += 1; for _ in 0..n { skip_value(data, pos); } }
        0xDC => { let n = u16::from_be_bytes([data[*pos+1], data[*pos+2]]) as usize; *pos += 3; for _ in 0..n { skip_value(data, pos); } }
        0xDE => { let n = u16::from_be_bytes([data[*pos+1], data[*pos+2]]) as usize; *pos += 3; for _ in 0..n { skip_value(data, pos); skip_value(data, pos); } }
        _ if b <= 0x7F => *pos += 1,
        _ => *pos += 1,
    }
}
