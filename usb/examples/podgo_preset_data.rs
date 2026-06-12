// Full Connect mode: establish x1, x80, x2 channels, request preset data, ACK and parse
use std::time::Duration;
use anyhow::{anyhow, Result};
use rusb::{Context, UsbContext};

const VID: u16 = 0x0e41;
const PID: u16 = 0x4247;
const EP_OUT: u8 = 0x01;
const EP_IN: u8 = 0x81;

fn drain(handle: &rusb::DeviceHandle<Context>) {
    let mut buf = [0u8; 512];
    loop { match handle.read_bulk(EP_IN, &mut buf, Duration::from_millis(30)) { Ok(n) => eprintln!("  drain: {n} bytes"), Err(_) => break } }
}

fn xfer(handle: &rusb::DeviceHandle<Context>, data: &[u8], label: &str, ms: u64) -> Result<Vec<u8>> {
    let mut buf = [0u8; 4096];
    handle.write_bulk(EP_OUT, data, Duration::from_millis(ms))?;
    let n = handle.read_bulk(EP_IN, &mut buf, Duration::from_millis(ms))?;
    let resp = buf[..n].to_vec();
    let channel = if n > 7 {
        if buf[4]==0x01 && buf[5]==0x10 { "x1" }
        else if buf[4]==0x80 && buf[5]==0x10 { "x80" }
        else if buf[4]==0x02 && buf[5]==0x10 { "x2" }
        else { "??" }
    } else { "?" };
    let hex: Vec<String> = resp[..n.min(32)].iter().map(|b| format!("{b:02x}")).collect();
    println!("  [{channel:>3}] {label}: {n}b {:?}", hex);
    Ok(resp)
}

fn try_read(handle: &rusb::DeviceHandle<Context>, label: &str, ms: u64) -> Result<Option<Vec<u8>>> {
    let mut buf = [0u8; 4096];
    match handle.read_bulk(EP_IN, &mut buf, Duration::from_millis(ms)) {
        Ok(n) => Ok(Some(buf[..n].to_vec())),
        Err(rusb::Error::Timeout) => Ok(None),
        Err(e) => anyhow::bail!("{label}: {e}"),
    }
}

fn extract_text(data: &[u8]) -> Vec<String> {
    let mut texts = vec![];
    let mut i = 0;
    while i < data.len() {
        if data[i] >= 0x20 && data[i] < 0x7f {
            let start = i;
            while i < data.len() && data[i] >= 0x20 && data[i] < 0x7f { i += 1; }
            let s: String = data[start..i].iter().map(|&c| c as char).collect();
            if s.len() >= 2 && !s.starts_with("da") { texts.push(s); }
        } else {
            i += 1;
        }
    }
    texts
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

    // ============ Phase 1: x1 session with sub_type=5 ============
    println!("\n=== Phase 1: x1 session (sub_type=5) ===");
    xfer(&h, &[0x0C,0,0,0x28,1,0x10,0xEF,3,0,0,0,2,0,1,0,0x21,0,0x10,0,0], "HANDSHAKE", 3000)?;
    xfer(&h, &[0x11,0,0,0x18,1,0x10,0xEF,3,0,2,0,4,0,0x10,0,0,1,0,5,0,1,0,0,0,5,0,0,0], "SESS_OPEN_1", 3000)?;
    xfer(&h, &[0x08,0,0,0x18,1,0x10,0xEF,3,0,3,0,8,0x20,0x10,0,0], "CHUNK_READ", 3000)?;
    xfer(&h, &[0x08,0,0,0x18,1,0x10,0xEF,3,0,4,0,2,0x20,0x10,0,0], "CMD_0002", 3000)?;

    // ============ Phase 2: x80 channel ============
    println!("\n=== Phase 2: x80 channel ===");
    xfer(&h, &[0x0C,0,0,0x28,0x80,0x10,0xED,3,0,0,0,2,0,1,0,0x21,0,0x10,0,0], "HANDSHAKE", 3000)?;
    xfer(&h, &[0x11,0,0,0x18,0x80,0x10,0xED,3,0,2,0,4,0,0x10,0,0,1,0,6,0,1,0,0,0,6,0,0,0], "SESS_OPEN_1", 3000)?;

    // ============ Phase 3: x2 channel ============
    println!("\n=== Phase 3: x2 channel ===");
    xfer(&h, &[0x0C,0,0,0x28,2,0x10,0xF0,3,0,0,0,2,0,1,0,0x21,0,0x10,0,0], "HANDSHAKE", 3000)?;
    xfer(&h, &[0x11,0,0,0x18,2,0x10,0xF0,3,0,2,0,4,0,0x10,0,0,1,0,4,0,1,0,0,0,4,0,0,0], "SESS_OPEN_1", 3000)?;

    // ============ Phase 4: Open resource 1000 on x80 ============
    println!("\n=== Phase 4: Open resource 1000 on x80 ===");
    xfer(&h, &[
        0x19,0,0,0x18,0x80,0x10,0xED,3,0,3,0,4,
        0x09,0x10,0,0,
        1,0,6,0,9,0,0,0,
        0x83,0x66,0xCD,3,0xE8,0x64,0x4C,0x65,0x80,0,0,0
    ], "OPEN_1000", 3000)?;

    // ============ Phase 5: Request preset data on x80 ============
    println!("\n=== Phase 5: Request preset data ===");
    let session_no: u8 = 0x0F;
    let mut dbl0: u8 = 0x10;
    let mut dbl1: u8 = 0x00;
    let mut seq: u8 = 4;
    let mut request_session_id: u8 = 0xF4;

    let r = xfer(&h, &[
        0x19,0,0,0x18,0x80,0x10,0xED,3,0,seq,0,0x0C,
        session_no, dbl0, dbl1, 0,
        1,0,6,0,9,0,0,0,
        0x83,0x66,0xCD,3,request_session_id,0x64,0x16,0x65,0xC0,0,0,0
    ], "REQ_PRESET", 3000)?;
    // After successful request, increment session_id for next time
    request_session_id = request_session_id.wrapping_add(2);

    // ============ Phase 6: Read all preset data chunks with ACK ============
    println!("\n=== Phase 6: Read data chunks ===");
    let mut all_data: Vec<u8> = vec![];

    for chunk in 0..60 {
        let timeout_ms = if chunk == 0 { 3000u64 } else { 2000 };
        match try_read(&h, &format!("chunk[{chunk}]"), timeout_ms)? {
            Some(data) => {
                if data.len() <= 16 {
                    println!("  End marker: {}b (done)", data.len());
                    break;
                }
                // Extract payload from byte 16
                if data.len() > 16 {
                    let payload = &data[16..];
                    all_data.extend_from_slice(payload);

                    // Find text in payload
                    let texts = extract_text(payload);
                    if !texts.is_empty() {
                        println!("  Chunk {chunk}: {}b (+{} payload) texts: {:?}",
                            data.len(), payload.len(), &texts[..texts.len().min(5)]);
                    } else {
                        println!("  Chunk {chunk}: {}b (+{} payload)", data.len(), payload.len());
                    }

                    // Send ACK
                    seq += 1;
                    dbl0 = dbl0.wrapping_add(1);
                    if dbl0 == 0 { dbl1 = dbl1.wrapping_add(1); }
                    let ack = &[
                        0x08,0,0,0x18,0x80,0x10,0xED,3,0,seq,0,8,
                        session_no, dbl0, dbl1, 0
                    ];
                    let _ = h.write_bulk(EP_OUT, ack, Duration::from_millis(100));
                }
            }
            None => {
                println!("  (no more data)");
                break;
            }
        }
    }

    println!("\n=== Summary ===");
    println!("Total accumulated data: {} bytes", all_data.len());

    // Find all text strings in the accumulated data
    let texts = extract_text(&all_data);
    println!("\nText strings found:");
    for t in &texts {
        if t.len() >= 2 {
            println!("  \"{t}\"");
        }
    }

    // Dump raw data for further parsing
    println!("\nRaw data hex dump (first 1024 bytes):");
    for (i, chunk) in all_data.chunks(32).enumerate() {
        if i > 31 { println!("  ..."); break; }
        let hex: Vec<String> = chunk.iter().map(|b| format!("{b:02x}")).collect();
        let ascii: String = chunk.iter().map(|&b| if b >= 0x20 && b < 0x7f { b as char } else { '.' }).collect();
        println!("  {:04x}: {:48} {}", i*32, hex.join(" "), ascii);
    }

    // Look for module structure patterns
    // snapshots typically start with 91 87 or similar markers
    if let Some(pos) = all_data.windows(2).position(|w| w == b"\x91\x87") {
        println!("\nFirst '91 87' marker at offset {pos}");
    }

    h.release_interface(0)?;
    println!("\nDone.");
    Ok(())
}
