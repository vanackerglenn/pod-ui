// POD Go write-protocol exploration harness (v3).
//
// The READ path (usb/src/current_preset.rs) is: connect x1/x80/x2 -> open
// resource 1000 (cmd 0x04) -> start paged stream of resource 1012 with access
// param 0x16 (cmd 0x0C) -> pull chunks (cmd 0x08). The WRITE path is unknown.
//
// v3 builds the 1012 open from the EXACT known-good bytes and only patches the
// fields we want to sweep (command, access param, mode flag) — they keep the
// packet length identical, so the wire format stays valid. Each probe runs on
// its own fresh connection so the device never desyncs.
//
//   open template (36B), patched positions marked:
//     19 00 00 18  80 10 ED 03  00 SEQ 00 CMD
//     0F 10 00 00  01 00 06 00  09 00 00 00
//     83 66 CD 03 F4 64 PARAM 65 EXTRA 00 00 00
//
// Phases:
//   * default: sanity-read 1012, then sweep cmd x param x extra (fresh connect
//     each) and print status bytes. Hunt for a status other than 0902 (normal
//     read ack) / 2b02 (invalid) that looks like a writable handle.
//   * `--write`: open 1012 with chosen cmd/param/extra (env-overridable) then
//     push the preset bytes. WATCH THE DEVICE; reload the patch to undo.
//
// Usage:
//   cargo run -p pod-usb --example podgo_write
//   PODGO_CMD=0x0c PODGO_PARAM=0x16 PODGO_EXTRA=0x01 cargo run ... -- --write

use std::time::Duration;
use anyhow::{anyhow, bail, Result};
use rusb::{Context, DeviceHandle, UsbContext};

const VID: u16 = 0x0e41;
const PID: u16 = 0x4247;
const EP_OUT: u8 = 0x01;
const EP_IN: u8 = 0x81;
const TIMEOUT: Duration = Duration::from_millis(1500);
const DRAIN_TIMEOUT: Duration = Duration::from_millis(40);

fn drain(h: &DeviceHandle<Context>) {
    let mut buf = [0u8; 512];
    while h.read_bulk(EP_IN, &mut buf, DRAIN_TIMEOUT).is_ok() {}
}

fn xfer(h: &DeviceHandle<Context>, data: &[u8]) -> Result<Vec<u8>> {
    let mut buf = [0u8; 4096];
    h.write_bulk(EP_OUT, data, TIMEOUT)?;
    let n = h.read_bulk(EP_IN, &mut buf, TIMEOUT)?;
    Ok(buf[..n].to_vec())
}

fn xfer_log(h: &DeviceHandle<Context>, label: &str, data: &[u8]) -> Result<Vec<u8>> {
    match xfer(h, data) {
        Ok(r) => {
            let head: Vec<String> = r[..r.len().min(16)].iter().map(|b| format!("{b:02x}")).collect();
            let status = if r.len() >= 14 { format!("status={:02x}{:02x}", r[12], r[13]) } else { "short".into() };
            println!("  {label:<36} {} bytes  {status}  [{}]", r.len(), head.join(" "));
            Ok(r)
        }
        Err(e) => { println!("  {label:<36} ERROR {e}"); Err(e) }
    }
}

fn open(ctx: &Context) -> Result<DeviceHandle<Context>> {
    let h = ctx.devices()?.iter()
        .find(|d| d.device_descriptor().map(|dd| dd.vendor_id() == VID && dd.product_id() == PID).unwrap_or(false))
        .ok_or(anyhow!("POD Go not found"))?
        .open()?;
    h.set_auto_detach_kernel_driver(true).ok();
    h.claim_interface(0)?;
    let _ = h.clear_halt(EP_OUT);
    let _ = h.clear_halt(EP_IN);
    drain(&h);
    Ok(h)
}

/// Full 3-channel connect + open resource 1000 (verbatim from current_preset.rs).
fn handshake(h: &DeviceHandle<Context>) -> Result<()> {
    xfer(h, &[0x0C,0,0,0x28,1,0x10,0xEF,3,0,0,0,2,0,1,0,0x21,0,0x10,0,0])?;
    xfer(h, &[0x11,0,0,0x18,1,0x10,0xEF,3,0,2,0,4,0,0x10,0,0,1,0,5,0,1,0,0,0,5,0,0,0])?;
    xfer(h, &[0x08,0,0,0x18,1,0x10,0xEF,3,0,3,0,8,0x20,0x10,0,0])?;
    xfer(h, &[0x08,0,0,0x18,1,0x10,0xEF,3,0,4,0,2,0x20,0x10,0,0])?;
    xfer(h, &[0x0C,0,0,0x28,0x80,0x10,0xED,3,0,0,0,2,0,1,0,0x21,0,0x10,0,0])?;
    xfer(h, &[0x11,0,0,0x18,0x80,0x10,0xED,3,0,2,0,4,0,0x10,0,0,1,0,6,0,1,0,0,0,6,0,0,0])?;
    xfer(h, &[0x0C,0,0,0x28,2,0x10,0xF0,3,0,0,0,2,0,1,0,0x21,0,0x10,0,0])?;
    xfer(h, &[0x11,0,0,0x18,2,0x10,0xF0,3,0,2,0,4,0,0x10,0,0,1,0,4,0,1,0,0,0,4,0,0,0])?;
    xfer(h, &[0x19,0,0,0x18,0x80,0x10,0xED,3,0,3,0,4, 0x09,0x10,0,0, 1,0,6,0,9,0,0,0,
              0x83,0x66,0xCD,3,0xE8,0x64,0x4C,0x65,0x80,0,0,0])?;
    Ok(())
}

fn fresh<T>(ctx: &Context, f: impl FnOnce(&DeviceHandle<Context>) -> Result<T>) -> Result<T> {
    let h = open(ctx)?;
    handshake(&h)?;
    let r = f(&h);
    let _ = h.release_interface(0);
    std::thread::sleep(Duration::from_millis(120));
    r
}

/// The exact known-good resource-1012 open, with cmd/param/extra patched in.
/// `extra` = 0xC0 means MessagePack nil.
fn build_open(seq: u8, cmd: u8, param: u8, extra: u8) -> Vec<u8> {
    vec![
        0x19,0,0,0x18, 0x80,0x10,0xED,3, 0,seq, 0,cmd,
        0x0F,0x10,0,0, 1,0,6,0, 9,0,0,0,
        0x83,0x66,0xCD,3,0xF4,0x64, param, 0x65, extra, 0,0,0,
    ]
}

/// Known-good read of resource 1012 (sanity that the connection works).
fn read_preset(h: &DeviceHandle<Context>) -> Result<Vec<u8>> {
    let r = xfer(h, &build_open(4, 0x0C, 0x16, 0xC0))?;
    let mut all = vec![];
    if r.len() > 16 { all.extend_from_slice(&r[16..]); }
    let mut seq: u8 = 4;
    loop {
        seq = seq.wrapping_add(1);
        let mut buf = [0u8; 4096];
        h.write_bulk(EP_OUT, &[0x08,0,0,0x18,0x80,0x10,0xED,3,0,seq,0,8, 0x0F,0x10,0,0], TIMEOUT)?;
        match h.read_bulk(EP_IN, &mut buf, TIMEOUT) {
            Ok(n) if n > 16 => all.extend_from_slice(&buf[16..n]),
            _ => break,
        }
    }
    Ok(all)
}

/// Decode the channel + seq + cmd + status from a response packet.
fn decode(r: &[u8]) -> String {
    if r.len() < 14 { return format!("short({}B)", r.len()); }
    let ch = match &r[4..8] {
        [0xED,0x03,0x80,0x10] => "x80",
        [0xF0,0x03,0x02,0x10] => "x2 ",
        [0xEF,0x03,0x01,0x10] => "x1 ",
        _ => "???",
    };
    format!("{ch} seq={:02x} cmd={:02x} status={:02x}{:02x} ({}B)", r[9], r[11], r[12], r[13], r.len())
}

/// Send `pkt`, then read every response the device emits (until a timeout),
/// decoding each. `our_seq` lets us flag a genuine echo of our request.
fn open_and_readall(h: &DeviceHandle<Context>, label: &str, pkt: &[u8], our_seq: u8) -> Result<()> {
    drain(h); // clear any handshake leftovers so we only see answers to THIS open
    h.write_bulk(EP_OUT, pkt, TIMEOUT)?;
    let mut buf = [0u8; 4096];
    let mut got = 0;
    print!("  {label:<32} ->");
    for _ in 0..6 {
        match h.read_bulk(EP_IN, &mut buf, Duration::from_millis(400)) {
            Ok(n) => {
                got += 1;
                let echo = if n > 9 && buf[9] == our_seq { "*OURS*" } else { "" };
                print!("  [{} {}]", decode(&buf[..n]), echo);
            }
            Err(_) => break,
        }
    }
    if got == 0 { print!("  (no response)"); }
    println!();
    Ok(())
}

/// Sweep the mode flag for resource 1012 — one fresh connect per row, reading
/// ALL responses so we can distinguish a real x80 answer from x2 leftovers.
fn probe(ctx: &Context) {
    println!("\n=== probe open(resource=1012): drain + open + read-all responses ===");
    println!("(* OURS* = a response whose seq echoes our request; x80 = our channel)");
    // cmd didn't matter last time; focus on the mode flag (key 101 / 'extra').
    let combos: &[(u8, u8, u8)] = &[
        (0x0C, 0x16, 0xC0), // the known read (extra=nil) — expect a data stream
        (0x0C, 0x16, 0x00),
        (0x0C, 0x16, 0x01),
        (0x0C, 0x16, 0x02),
        (0x0C, 0x16, 0x80),
        (0x04, 0x16, 0x00),
        (0x04, 0x16, 0xC0),
    ];
    let mut seq = 0x30u8;
    for &(cmd, param, extra) in combos {
        seq = seq.wrapping_add(1);
        let label = format!("cmd=0x{cmd:02x} param=0x{param:02x} extra=0x{extra:02x}");
        let pkt = build_open(seq, cmd, param, extra);
        let our_seq = seq;
        let _ = fresh(ctx, |h| open_and_readall(h, &label, &pkt, our_seq));
    }
}

/// A proper extended DATA packet (same framing as build_open): header + handle
/// + [1,0,6,0] + payload_len(LE) + payload + 4-byte pad. byte0 = total-11, so
/// the payload must be small enough that total stays <= 266 (hence 240B chunks).
fn build_data(seq: u8, cmd: u8, data: &[u8]) -> Vec<u8> {
    let mut p = vec![0u8,0,0,0x18, 0x80,0x10,0xED,3, 0,seq, 0,cmd, 0x0F,0x10,0,0, 1,0,6,0];
    p.extend_from_slice(&(data.len() as u32).to_le_bytes());
    p.extend_from_slice(data);
    while p.len() % 4 != 0 { p.push(0); }
    p[0] = (p.len().saturating_sub(11)) as u8;
    p
}

/// Hypothesised write-back: open 1012 (extra=0x00 = the mode the device acks),
/// then push the bytes as correctly-framed extended DATA packets, reading the
/// ack after each. `dcmd` (PODGO_DCMD) is the data-chunk command; the device
/// acked our earlier chunks with cmd=0x10, so try 0x10 and 0x08.
fn attempt_write(ctx: &Context, data: &[u8], cmd: u8, param: u8, extra: u8, dcmd: u8) -> Result<()> {
    println!("\n=== write-back: open cmd=0x{cmd:02x} param=0x{param:02x} extra=0x{extra:02x}, data cmd=0x{dcmd:02x} ({} bytes) ===", data.len());
    println!("WATCH THE DEVICE. Reload the patch afterwards to restore the edit buffer.");
    fresh(ctx, |h| {
        let open = build_open(0x40, cmd, param, extra);
        let r = xfer(h, &open)?;
        println!("  open-for-write -> [{}]", decode(&r));
        let mut seq = 0x41u8;
        let mut buf = [0u8; 4096];
        let mut kseq = 0x05u8; // x80 keep-alive seq (continues handshake's x80 seq)
        for (i, chunk) in data.chunks(240).enumerate() {
            h.write_bulk(EP_OUT, &build_data(seq, dcmd, chunk), TIMEOUT)?;
            // Read EVERY response this chunk produces (not just one).
            let mut resps = vec![];
            loop {
                match h.read_bulk(EP_IN, &mut buf, Duration::from_millis(300)) {
                    Ok(n) => resps.push(decode(&buf[..n])),
                    Err(_) => break,
                }
            }
            if resps.is_empty() {
                // Stalled — try an x80 keep-alive (the read protocol pulls between
                // ops); see if the device then resumes / asks for something.
                kseq = kseq.wrapping_add(1);
                h.write_bulk(EP_OUT, &[0x08,0,0,0x18,0x80,0x10,0xED,3,0,kseq,0,8,0x0F,0x10,0,0], TIMEOUT)?;
                let ka = match h.read_bulk(EP_IN, &mut buf, Duration::from_millis(400)) {
                    Ok(n) => decode(&buf[..n]), Err(_) => "(none)".into(),
                };
                println!("  chunk {i:>2} ({}B) -> STALL; keep-alive -> [{ka}]", chunk.len());
            } else if i < 5 || i % 4 == 0 || resps.len() != 1 {
                println!("  chunk {i:>2} ({}B) -> {:?}", chunk.len(), resps);
            }
            seq = seq.wrapping_add(1);
        }
        println!("  (done pushing {} chunks)", data.chunks(240).count());
        Ok(())
    })
}

fn env_u8(key: &str, default: u8) -> u8 {
    std::env::var(key).ok()
        .and_then(|s| u8::from_str_radix(s.trim().trim_start_matches("0x"), 16).ok())
        .unwrap_or(default)
}

fn main() -> Result<()> {
    let do_write = std::env::args().any(|a| a == "--write");
    let ctx = Context::new()?;

    let preset = fresh(&ctx, read_preset)?;
    println!("sanity read of current preset: {} bytes", preset.len());
    if preset.is_empty() { bail!("empty read — is the device on, idle, and not held by pod-gui?"); }

    probe(&ctx);

    if do_write {
        attempt_write(&ctx, &preset,
            env_u8("PODGO_CMD", 0x0C), env_u8("PODGO_PARAM", 0x16),
            env_u8("PODGO_EXTRA", 0x00), env_u8("PODGO_DCMD", 0x10))?;
    } else {
        println!("\n(omit --write to stay read-only; pass --write to attempt a write-back)");
    }
    Ok(())
}
