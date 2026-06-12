// Dynamic Pod Go protocol fuzzer - tries different parameter combinations
// Usage: sudo ./podgo_dynamic [mode]
//   mode: probe_seq   - try CMD_0002 with diff seq values
//         probe_sub   - try SESSION_OPEN_1 with diff sub-type values
//         probe_res   - try opening resources with diff handle values
//         full_connect - full Connect mode with all channels

use std::time::Duration;
use std::env;
use anyhow::{anyhow, Result};
use rusb::{Context, UsbContext};

const VID: u16 = 0x0e41;
const PID: u16 = 0x4247;
const EP_OUT: u8 = 0x01;
const EP_IN: u8 = 0x81;

fn drain(handle: &rusb::DeviceHandle<Context>) {
    let mut buf = [0u8; 512];
    loop {
        match handle.read_bulk(EP_IN, &mut buf, Duration::from_millis(30)) {
            Ok(n) => eprintln!("  drain: {} bytes", n),
            Err(_) => break,
        }
    }
}

fn xfer(handle: &rusb::DeviceHandle<Context>, data: &[u8], timeout_ms: u64)
    -> std::result::Result<Vec<u8>, rusb::Error>
{
    let mut buf = [0u8; 4096];
    handle.write_bulk(EP_OUT, data, Duration::from_millis(timeout_ms))?;
    let n = handle.read_bulk(EP_IN, &mut buf, Duration::from_millis(timeout_ms))?;
    Ok(buf[..n].to_vec())
}

fn try_read(handle: &rusb::DeviceHandle<Context>, timeout_ms: u64)
    -> std::result::Result<Option<Vec<u8>>, rusb::Error>
{
    let mut buf = [0u8; 4096];
    match handle.read_bulk(EP_IN, &mut buf, Duration::from_millis(timeout_ms)) {
        Ok(n) => Ok(Some(buf[..n].to_vec())),
        Err(rusb::Error::Timeout) => Ok(None),
        Err(e) => Err(e),
    }
}

fn dump(prefix: &str, data: &[u8]) {
    let hex: Vec<String> = data.iter().map(|b| format!("{b:02x}")).collect();
    let tag = if data.len() > 7 {
        if data[4]==0x01 && data[5]==0x10 && data[6]==0xEF && data[7]==0x03 { "x1" }
        else if data[4]==0x80 && data[5]==0x10 && data[6]==0xED && data[7]==0x03 { "x80" }
        else if data[4]==0x02 && data[5]==0x10 && data[6]==0xF0 && data[7]==0x03 { "x2" }
        else { "??" }
    } else { "??" };
    println!("  {} [{}] {}", prefix, tag, hex.join(" "));
}

fn do_xfer(handle: &rusb::DeviceHandle<Context>, data: &[u8], label: &str) -> Result<Vec<u8>> {
    let resp = xfer(handle, data, 3000)
        .map_err(|e| anyhow!("{label}: {e}"))?;
    dump(label, &resp);
    Ok(resp)
}

fn do_try_read(handle: &rusb::DeviceHandle<Context>, label: &str, ms: u64) -> Result<Option<Vec<u8>>> {
    match try_read(handle, ms) {
        Ok(Some(r)) => { dump(label, &r); Ok(Some(r)) }
        Ok(None) => Ok(None),
        Err(e) => anyhow::bail!("{label}: {e}"),
    }
}

fn poll_until(handle: &rusb::DeviceHandle<Context>, label: &str, max_polls: u32) -> Result<Vec<Vec<u8>>> {
    let mut results = vec![];
    for i in 0..max_polls {
        if let Some(r) = do_try_read(handle, &format!("{label}[{i}]"), 200)? {
            results.push(r);
        } else {
            break;
        }
    }
    Ok(results)
}

// ============ Handshake helpers ============

fn handshake_x1(handle: &rusb::DeviceHandle<Context>) -> Result<Vec<u8>> {
    do_xfer(handle, &[0x0C,0,0,0x28,1,0x10,0xEF,3,0,0,0,2,0,1,0,0x21,0,0x10,0,0], "x1:HANDSHAKE")
}

fn session_open_1(handle: &rusb::DeviceHandle<Context>, seq: u8, sub_type: u8, channel: &[u8;4]) -> Result<Vec<u8>> {
    // 17 bytes + 4 + 4 + 4 = 28 bytes total for a simple session
    let mut pkt = vec![0x11,0,0,0x18];
    pkt.extend_from_slice(channel);
    pkt.extend_from_slice(&[0, seq, 0, 4]);   // seq, cmd=4
    pkt.extend_from_slice(&[0,0x10,0,0]);     // object handle
    pkt.extend_from_slice(&[1,0,sub_type,0]);  // block type=1, sub_type
    pkt.extend_from_slice(&[1,0,0,0]);         // payload length = 1
    pkt.extend_from_slice(&[sub_type,0,0,0]);  // payload value = sub_type
    assert_eq!(pkt.len(), 28);
    do_xfer(handle, &pkt, "SESS_OPEN_1")
}

fn session_open_1_ext(handle: &rusb::DeviceHandle<Context>, seq: u8, sub_type: u8,
    channel: &[u8;4], handle_obj: u32) -> Result<Vec<u8>>
{
    let mut pkt = vec![0x11,0,0,0x18];
    pkt.extend_from_slice(channel);
    pkt.extend_from_slice(&[0, seq, 0, 4]);
    pkt.extend_from_slice(&handle_obj.to_le_bytes());
    pkt.extend_from_slice(&[1,0,sub_type,0]);
    pkt.extend_from_slice(&[1,0,0,0]);
    pkt.extend_from_slice(&[sub_type,0,0,0]);
    do_xfer(handle, &pkt, "SESS_OPEN_1_EXT")
}

fn chunk_read(handle: &rusb::DeviceHandle<Context>, seq: u8, offset: u32) -> Result<Vec<u8>> {
    let o = offset.to_le_bytes();
    do_xfer(handle, &[0x08,0,0,0x18,1,0x10,0xEF,3,0,seq,0,8, o[0],o[1],o[2],o[3]], "CHUNK_READ")
}

fn cmd_0002(handle: &rusb::DeviceHandle<Context>, seq: u8) -> Result<Vec<u8>> {
    do_xfer(handle, &[0x08,0,0,0x18,1,0x10,0xEF,3,0,seq,0,2,0x20,0x10,0,0], "CMD_0002")
}

fn session_open_2(handle: &rusb::DeviceHandle<Context>, seq: u8, handle_obj: u32) -> Result<Vec<u8>> {
    let h = handle_obj.to_le_bytes();
    do_xfer(handle, &[
        0x1A,0,0,0x18,1,0x10,0xEF,3,0,seq,0,4,
        h[0],h[1],h[2],h[3],
        1,0,2,0,0x0A,0,0,0,
        0x83,0x66,0xCD,3,0xE8,0x64,0xCC,0xFE,0x65,0x80,0,0
    ], "SESS_OPEN_2")
}

fn open_resource(handle: &rusb::DeviceHandle<Context>, seq: u8,
    handle_obj: u32, rsc: u16, channel: &[u8;4], param: u8) -> Result<Vec<u8>>
{
    let h = handle_obj.to_le_bytes();
    let r = rsc.to_le_bytes();
    do_xfer(handle, &[
        0x19,0,0,0x18,
        channel[0],channel[1],channel[2],channel[3],
        0,seq,0,4,
        h[0],h[1],h[2],h[3],
        1,0,2,0,9,0,0,0,
        0x83,0x66,0xCD,3,r[0],r[1],0x64,param,0x65,0xC0,0,0,0
    ], &format!("OPEN_RSC_{rsc}_p{param}"))
}

fn open_stream(handle: &rusb::DeviceHandle<Context>, seq: u8,
    handle_obj: u32, rsc: u16) -> Result<Vec<u8>>
{
    let h = handle_obj.to_le_bytes();
    let r = rsc.to_le_bytes();
    do_xfer(handle, &[
        0x1D,0,0,0x18,1,0x10,0xEF,3,0,seq,0,0x0C,
        h[0],h[1],h[2],h[3],
        1,0,2,0,0x0D,0,0,0,
        0x83,0x66,0xCD,3,
        0xEA,0x64,1,0x65,0x82,0x6B,0,0x65,2,0,0,0
    ], &format!("OPEN_STR_{rsc}"))
}

// ============ Modes ============

fn probe_seq(handle: &rusb::DeviceHandle<Context>) -> Result<()> {
    // Try CMD_0002 with different seq values
    drain(handle);
    handshake_x1(handle)?;
    session_open_1(handle, 2, 2, &[1,0x10,0xEF,3])?;
    chunk_read(handle, 3, 0x1009)?;

    for seq in 0..=8u8 {
        println!("\n--- CMD_0002 with seq={} ---", seq);
        match cmd_0002(handle, seq) {
            Ok(r) => {
                if r.len() > 4 && r[12] == 0x09 && r[13] == 0x02 {
                    println!("  SUCCESS! Valid response with seq={}", seq);
                }
                // Try a read after to see if more data comes
                if let Ok(Some(more)) = try_read(handle, 100) {
                    dump("extra_after", &more);
                    // Check if it's x80/x2
                    if more.len() > 7 {
                        if more[4]==0x80 { println!("  => x80 found after cmd seq={seq}!"); }
                        if more[4]==0x02 { println!("  => x2 found after cmd seq={seq}!"); }
                    }
                }
            }
            Err(e) => println!("  seq={}: {e}", seq),
        }
        // Reset needed after timeout
        if seq < 8 {
            drain(handle);
            // Re-handshake for next try
            handshake_x1(handle)?;
            session_open_1(handle, 2, 2, &[1,0x10,0xEF,3])?;
            chunk_read(handle, 3, 0x1009)?;
        }
    }
    Ok(())
}

fn probe_sub(handle: &rusb::DeviceHandle<Context>) -> Result<()> {
    // Try SESSION_OPEN_1 with different sub_type values
    drain(handle);
    handshake_x1(handle)?;

    for sub in 1..=6u8 {
        println!("\n--- SESSION_OPEN_1 with sub_type={} ---", sub);
        match session_open_1(handle, 2, sub, &[1,0x10,0xEF,3]) {
            Ok(r) => {
                if r.len() > 20 {
                    // Extract the interesting part
                    if r.len() > 16 {
                        let extra: Vec<String> = r[16..].iter().map(|b| format!("{b:02x}")).collect();
                        println!("  Response body: {}", extra.join(" "));
                        // Check for "P34" or "Main" text
                        if r.windows(4).any(|w| w == b"P34M" || w == b"Main") {
                            println!("  => Contains 'P34Main' text!");
                        }
                    }
                }
            }
            Err(e) => println!("  sub_type={}: {e}", sub),
        }
        drain(handle);
        handshake_x1(handle)?;
    }
    Ok(())
}

fn probe_res_after_sub5(handle: &rusb::DeviceHandle<Context>) -> Result<()> {
    drain(handle);
    handshake_x1(handle)?;
    session_open_1(handle, 2, 5, &[1,0x10,0xEF,3])?;
    chunk_read(handle, 3, 0x1020)?;

    // CMD_0002 seq=4
    cmd_0002(handle, 4)?;

    // Skip SESSION_OPEN_2 — try resources on the session handle 0x100A directly
    // Handle 0x100A is the session handle from SESSION_OPEN_1 (seq=2, cmd=4)
    // Try with handle=0x101A (session 2 handle) and 0x100A (session 1 handle)
    for (try_i, &handle_val) in [0x101Au32, 0x100A].iter().enumerate() {
        println!("\n--- Trying handle 0x{handle_val:04x} (try {try_i}) ---");

        for (i, &(rsc, param)) in [
            (1001u16, 0u8), (1001, 21),
            (1003, 0), (1003, 21), (1003, 1),
            (1004, 0), (1004, 21), (1004, 1),
            (1012, 0), (1012, 21),
            (1013, 0), (1013, 21),
            (1005, 0), (1006, 0),
        ].iter().enumerate() {
            let rsc_seq = 5 + i as u8;
            match open_resource(handle, rsc_seq, handle_val, rsc, &[1,0x10,0xEF,3], param) {
                Ok(r) => {
                    if r.len() > 12 {
                        let status = (r[12], r[13]);
                        let extra = if r.len() > 16 {
                            let e: Vec<String> = r[16..r.len().min(32)].iter().map(|b| format!("{b:02x}")).collect();
                            format!(" [{}]", e.join(" "))
                        } else { String::new() };
                        println!("  RSC {rsc} p{param} h=0x{handle_val:04x}: {:02x}{:02x}{}{}", r[12], r[13],
                            if status == (0x09,0x02) { " OK" } else { "" }, extra);
                    }
                }
                Err(e) => println!("  RSC {rsc} p{param} h=0x{handle_val:04x}: {e}"),
            }
        }
    }

    Ok(())
}

fn full_connect(handle: &rusb::DeviceHandle<Context>) -> Result<()> {
    // Full Connect mode: x1 handshake with sub_type=5,
    // then wait for device to initiate x80/x2
    drain(handle);
    handshake_x1(handle)?;
    session_open_1(handle, 2, 5, &[1,0x10,0xEF,3])?;
    chunk_read(handle, 3, 0x1020)?;

    // Try CMD_0002 with seq=3,4,5
    for seq in 3..=7u8 {
        println!("\n--- CMD_0002 seq={} then poll for x80/x2 ---", seq);
        match cmd_0002(handle, seq) {
            Ok(r) => {
                // After CMD_0002 succeeds, poll for device-initiated messages
                for poll in 0..20 {
                    match try_read(handle, 200) {
                        Ok(Some(msg)) => {
                            dump("poll", &msg);
                            if msg.len() > 7 {
                                if msg[4]==0x80 { println!("  *** x80 CHANNEL FOUND! ***"); }
                                if msg[4]==0x02 { println!("  *** x2 CHANNEL FOUND! ***"); }
                                    // Respond with x80 SESSION_OPEN_1
                                    let x80: [u8;4] = [0x80,0x10,0xED,3];
                                    match session_open_1(handle, 5, 6, &x80) {
                                        Ok(r) => {
                                            dump("x80_RESP", &r);
                                            // Now try reading preset data on x80
                                            // Open resource 1012 on x80
                                            let x80_ch: [u8;4] = [0x80,0x10,0xED,3];
                                            match open_resource(handle, 6, 0x101A, 1012, &x80_ch, 0) {
                                                Ok(r) => {
                                                    dump("x80:RSC_1012", &r);
                                                    // Try opening stream
                                                    // open_stream(handle, 7, 0x101A, 1012);
                                                }
                                                Err(e) => println!("  x80 open resource 1012: {e}"),
                                            }
                                        }
                                        Err(e) => println!("  x80 SESSION_OPEN_1: {e}"),
                                    }
                            }
                        }
                        Ok(None) => {
                            if poll > 5 { break; }
                        }
                        Err(e) => anyhow::bail!("poll: {e}"),
                    }
                }
            }
            Err(e) => println!("  CMD seq={}: {e}", seq),
        }
    }
    Ok(())
}

fn main() -> Result<()> {
    let args: Vec<String> = env::args().collect();
    let mode = args.get(1).map(|s| s.as_str()).unwrap_or("probe_seq");

    let ctx = Context::new()?;
    let handle = {
        let devices = ctx.devices()?;
        let mut found = None;
        for dev in devices.iter() {
            let d = dev.device_descriptor()?;
            if d.vendor_id() == VID && d.product_id() == PID {
                found = Some(dev.open()?); break;
            }
        }
        found.ok_or(anyhow!("Pod Go not found"))?
    };
    handle.set_auto_detach_kernel_driver(true)?;
    handle.claim_interface(0)?;
    handle.clear_halt(EP_OUT)?;
    handle.clear_halt(EP_IN)?;

    match mode {
        "probe_seq" => probe_seq(&handle)?,
        "probe_sub" => probe_sub(&handle)?,
        "probe_res" => probe_res_after_sub5(&handle)?,
        "full_connect" => full_connect(&handle)?,
        "midi_pc" => {
            // Release iface 0, claim iface 4 (MIDI), send PC, re-claim 0, try reading
            handle.release_interface(0)?;
            handle.claim_interface(4)?;
            let midi_out = 0x02;
            let midi_in = 0x82;
            // Send PC 0 (preset 0)
            let pc = [0xC0, 0]; // Program Change, channel 1
            handle.write_bulk(midi_out, &pc, Duration::from_secs(1))?;
            println!("  Sent PC 0 on MIDI iface 4");
            std::thread::sleep(Duration::from_millis(500));
            handle.release_interface(4)?;
            // Re-claim iface 0
            handle.claim_interface(0)?;
            handle.clear_halt(EP_OUT)?;
            handle.clear_halt(EP_IN)?;
            drain(&handle);

            // Now try reading current preset via bulk protocol
            handshake_x1(&handle)?;
            session_open_1(&handle, 2, 2, &[1,0x10,0xEF,3])?;
            chunk_read(&handle, 3, 0x1009)?;
            cmd_0002(&handle, 4)?;
            session_open_2(&handle, 5, 0x100A)?;
            chunk_read(&handle, 6, 0x101A)?;

            // Open resource 1003 (maybe current preset?)
            for (i, rsc) in [1001u16, 1003, 1004, 1012, 1013].iter().enumerate() {
                let seq = 7 + i as u8;
                match open_resource(&handle, seq, 0x101A, *rsc, &[1,0x10,0xEF,3], 0) {
                    Ok(r) => {
                        let status = if r.len() > 12 { (r[12], r[13]) } else { (0,0) };
                        println!("  PC0 after: RSC {rsc}: {:02x}{:02x}{}",
                            r[12], r[13], if status == (0x09,0x02) { " OK" } else { "" });
                        if r.len() > 20 {
                            let hex: Vec<String> = r[16..r.len().min(64)].iter().map(|b| format!("{b:02x}")).collect();
                            println!("    payload: {}", hex.join(" "));
                        }
                    }
                    Err(e) => println!("  PC0 after: RSC {rsc}: {e}"),
                }
            }

            // Try open_stream on 1003
            let prev = open_stream(&handle, 12, 0x101A, 1003);
            match prev {
                Ok(r) => println!("  Stream 1003: {} bytes", r.len()),
                Err(e) => println!("  Stream 1003: {e}"),
            }
        }
        _ => println!("Unknown mode: {mode}. Use: probe_seq, probe_sub, probe_res, full_connect, midi_pc"),
    }

    handle.release_interface(0)?;
    Ok(())
}
