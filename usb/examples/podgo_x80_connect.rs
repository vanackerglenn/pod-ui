// Full Connect mode: establish x1, x80, and x2 channels on Pod Go
// Then request preset data on x80
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
    let hex: Vec<String> = resp[..n.min(48)].iter().map(|b| format!("{b:02x}")).collect();
    println!("  [{channel:>3}] {label}: {hex:?}");
    Ok(resp)
}

fn try_read(handle: &rusb::DeviceHandle<Context>, label: &str, ms: u64) -> Result<Option<Vec<u8>>> {
    let mut buf = [0u8; 4096];
    match handle.read_bulk(EP_IN, &mut buf, Duration::from_millis(ms)) {
        Ok(n) => {
            let channel = if buf[4]==0x01 && buf[5]==0x10 { "x1" }
                else if buf[4]==0x80 && buf[5]==0x10 { "x80" }
                else if buf[4]==0x02 && buf[5]==0x10 { "x2" }
                else { "??" };
            let hex: Vec<String> = buf[..n.min(48)].iter().map(|b| format!("{b:02x}")).collect();
            println!("  [{channel:>3}] {label}: read {n}b: {hex:?}");
            Ok(Some(buf[..n].to_vec()))
        }
        Err(rusb::Error::Timeout) => Ok(None),
        Err(e) => anyhow::bail!("{label}: {e}"),
    }
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

    // seq=0: HANDSHAKE on x1
    let r = xfer(&h, &[0x0C,0,0,0x28,1,0x10,0xEF,3,0,0,0,2,0,1,0,0x21,0,0x10,0,0], "x1 HANDSHAKE", 3000)?;
    println!("    Response seq={} cmd={:02x}{:02x} status={:02x}{:02x}", r[8], r[11], r[10], r[12], r[13]);

    // seq=2: SESSION_OPEN_1 on x1 with sub_type=5
    let r = xfer(&h, &[0x11,0,0,0x18,1,0x10,0xEF,3,0,2,0,4,0,0x10,0,0,1,0,5,0,1,0,0,0,5,0,0,0], "x1 SESS_OPEN_1", 3000)?;
    println!("    Response seq={} status={:02x}{:02x} len={}", r[8], r[12], r[13], r.len());
    if r.len() > 24 {
        let rest: Vec<String> = r[24..r.len().min(68)].iter().map(|b| format!("{b:02x}")).collect();
        println!("    Payload: {:?}", rest);
        // Check for P34Main text
        if let Some(pos) = r.windows(7).position(|w| w == b"P34Main") {
            println!("    Contains 'P34Main' at offset {pos}");
        }
    }

    // seq=3: CHUNK_READ offset=0x1020
    let r = xfer(&h, &[0x08,0,0,0x18,1,0x10,0xEF,3,0,3,0,8,0x20,0x10,0,0], "x1 CHUNK_READ", 3000)?;
    println!("    Response seq={} status={:02x}{:02x}", r[8], r[12], r[13]);

    // seq=4: CMD_0002 — this tells device we want more channels
    let r = xfer(&h, &[0x08,0,0,0x18,1,0x10,0xEF,3,0,4,0,2,0x20,0x10,0,0], "x1 CMD_0002", 3000)?;
    println!("    Response seq={} cmd={:02x}{:02x} status={:02x}{:02x}", r[8], r[11], r[10], r[12], r[13]);

    // ============ Phase 2: Initiate x80 channel ============
    println!("\n=== Phase 2: x80 channel (HOST initiates) ===");

    // seq=0 on x80: HANDSHAKE (HOST sends this, device doesn't send it)
    let r = xfer(&h, &[0x0C,0,0,0x28,0x80,0x10,0xED,3,0,0,0,2,0,1,0,0x21,0,0x10,0,0], "x80 HANDSHAKE", 3000)?;
    println!("    Response seq={} status={:02x}{:02x}", r[8], r[12], r[13]);
    let is_x80_ack = r.len() > 7 && r[4] == 0x80;
    println!("    Is x80 response: {is_x80_ack}");

    // seq=2 on x80: SESSION_OPEN_1 with sub_type=6
    let r = xfer(&h, &[0x11,0,0,0x18,0x80,0x10,0xED,3,0,2,0,4,0,0x10,0,0,1,0,6,0,1,0,0,0,6,0,0,0], "x80 SESS_OPEN_1", 3000)?;
    println!("    Response seq={} status={:02x}{:02x}", r[8], r[12], r[13]);
    let is_x80_resp = r.len() > 7 && r[4] == 0x80;
    println!("    Is x80 response: {is_x80_resp}");

    // Check if device sent more data (maybe x2 handshake spontaneously)
    for i in 0..5 {
        if let Ok(Some(msg)) = try_read(&h, &format!("poll[{i}]"), 200) {
            println!("    Device sent unsolicited data on poll {i}");
        } else {
            break;
        }
    }

    // ============ Phase 3: Initiate x2 channel ============
    println!("\n=== Phase 3: x2 channel (HOST initiates) ===");

    // seq=0 on x2: HANDSHAKE
    let r = xfer(&h, &[0x0C,0,0,0x28,2,0x10,0xF0,3,0,0,0,2,0,1,0,0x21,0,0x10,0,0], "x2 HANDSHAKE", 3000)?;
    println!("    Response seq={} status={:02x}{:02x}", r[8], r[12], r[13]);
    let is_x2_ack = r.len() > 7 && r[4] == 0x02;
    println!("    Is x2 response: {is_x2_ack}");

    // seq=2 on x2: SESSION_OPEN_1 with sub_type=4
    let r = xfer(&h, &[0x11,0,0,0x18,2,0x10,0xF0,3,0,2,0,4,0,0x10,0,0,1,0,4,0,1,0,0,0,4,0,0,0], "x2 SESS_OPEN_1", 3000)?;
    println!("    Response seq={} status={:02x}{:02x}", r[8], r[12], r[13]);
    let is_x2_resp = r.len() > 7 && r[4] == 0x02;
    println!("    Is x2 response: {is_x2_resp}");

    // After x2 SESSION_OPEN_1, Connect mode also opens resource 1000 on x80
    // The device response with seq=2 on x2 triggers: start x2 keep-alive
    // Then host opens resource 1000 on x80

    // Check for more device data
    for i in 0..5 {
        if let Ok(Some(msg)) = try_read(&h, &format!("poll[{i}]"), 200) {
            println!("    Device sent data on poll {i}");
        } else {
            break;
        }
    }

    // ============ Phase 4: Open resource 1000 on x80 ============
    println!("\n=== Phase 4: Open resource 1000 on x80 ===");
    // This is from the Connect mode's x2 response handler:
    // data = [0x19, ..., 0x80, ..., 0x83, 0x66, 0xcd, 3, 0xe8, 0x64, 0x4c, 0x65, 0x80, 0, 0, 0]
    // resource=0x3E8=1000, param=0x4c=76
    let r = xfer(&h, &[
        0x19,0,0,0x18,0x80,0x10,0xED,3,0,3,0,4,
        0x09,0x10,0,0,  // handle from Connect mode (0x1009)
        1,0,6,0,9,0,0,0,
        0x83,0x66,0xCD,3,0xE8,0x64,0x4C,0x65,0x80,0,0,0
    ], "x80 OPEN_1000", 3000)?;
    println!("    Response: {}b status={:02x}{:02x}", r.len(), r[12], r[13]);

    // ============ Phase 5: Request preset data on x80 ============
    println!("\n=== Phase 5: Request preset data on x80 ===");

    // Now try RequestPreset mode on x80
    // Open resource 1012 (0x3F4) on x80 with param=22 (0x16)
    // Use maybe_session_no = 0x0F (random)
    // Use next_packet_double = [0x10, 0x00] (counter)
    println!("\n  -- Request preset data (resource 1012 on x80) --");

    // From RequestPreset.start():
    // cmd = 0x0C (open and start streaming)
    // resource = 0x3F4 = 1012
    // param = 0x16 = 22
    // handle = maybe_session_no, next_packet_double[0], next_packet_double[1]
    let maybe_session_no: u8 = 0x0F;
    let dbl0: u8 = 0x10;
    let dbl1: u8 = 0x00;
    let r = xfer(&h, &[
        0x19,0,0,0x18,0x80,0x10,0xED,3,0,4,0,0x0C,
        maybe_session_no, dbl0, dbl1, 0,
        1,0,6,0,9,0,0,0,
        0x83,0x66,0xCD,3,0xF4,0x64,0x16,0x65,0xC0,0,0,0
    ], "x80 REQ_PRESET", 3000)?;
    println!("    Response: {}b", r.len());
    if r.len() > 12 {
        let hex: Vec<String> = r[..r.len().min(64)].iter().map(|b| format!("{b:02x}")).collect();
        println!("    Response hex: {:?}", hex);
    }

    // Send keep-alive on x1 to maintain session
    println!("\n  -- Sending x1 keep-alive --");
    let _ = xfer(&h, &[0x08,0,0,0x18,1,0x10,0xEF,3,0,5,0,8,0x72,0x1E,0,0], "x1 KEEPALIVE", 1000);

    // Send keep-alive on x80
    println!("\n  -- Sending x80 keep-alive --");
    let _ = xfer(&h, &[0x08,0,0,0x18,0x80,0x10,0xED,3,0,5,0,0x10,0x09,0x10,0,0], "x80 KEEPALIVE", 1000);

    // Keep-alive on x2
    println!("\n  -- Sending x2 keep-alive --");
    let _ = xfer(&h, &[0x08,0,0,0x18,0x02,0x10,0xF0,3,0,3,0,0x10,0x09,0x10,0,0], "x2 KEEPALIVE", 1000);

    // Poll for response data on any channel
    println!("\n  -- Polling for data --");
    for i in 0..20 {
        match try_read(&h, &format!("poll[{i}]"), 500) {
            Ok(Some(r)) => {
                if r.len() > 12 {
                    let hex: Vec<String> = r[..r.len().min(64)].iter().map(|b| format!("{b:02x}")).collect();
                    println!("    [{i}] {:?}", hex);
                }
            }
            Ok(None) => {
                if i > 3 { println!("    (idle)"); break; }
            }
            Err(e) => { println!("    [{i}] error: {e}"); break; }
        }
    }

    h.release_interface(0)?;
    println!("\nDone.");
    Ok(())
}
