// Pod Go Current Preset Probe
// Reads the current active preset data from Pod Go via HX USB bulk protocol
// Uses full 3-channel connect (x1 + x80 + x2) to request preset data on x80
// Outputs: module names, one per line, followed by hex dump of raw data
//
// Usage: sudo podgo_current_probe
// Output:
//   MODULE: Deez One Vintage
//   MODULE: Mono FX Loop
//   ...
//   SNAPSHOT: SNAPSHOT 1
//   SNAPSHOT: SNAPSHOT 2
//   ...
//   HEX: <full hex dump of raw preset data>

use std::time::Duration;
use std::process;
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

fn xfer(handle: &rusb::DeviceHandle<Context>, data: &[u8], _label: &str, ms: u64) -> Result<Vec<u8>> {
    let mut buf = [0u8; 4096];
    handle.write_bulk(EP_OUT, data, Duration::from_millis(ms))?;
    let n = handle.read_bulk(EP_IN, &mut buf, Duration::from_millis(ms))?;
    Ok(buf[..n].to_vec())
}

fn main() {
    if let Err(e) = run() {
        eprintln!("Error: {e}");
        process::exit(1);
    }
}

fn run() -> Result<()> {
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

    // Phase 1: x1 session (sub_type=5)
    xfer(&h, &[0x0C,0,0,0x28,1,0x10,0xEF,3,0,0,0,2,0,1,0,0x21,0,0x10,0,0], "H1", 3000)?;
    xfer(&h, &[0x11,0,0,0x18,1,0x10,0xEF,3,0,2,0,4,0,0x10,0,0,1,0,5,0,1,0,0,0,5,0,0,0], "H2", 3000)?;
    xfer(&h, &[0x08,0,0,0x18,1,0x10,0xEF,3,0,3,0,8,0x20,0x10,0,0], "H3", 3000)?;
    xfer(&h, &[0x08,0,0,0x18,1,0x10,0xEF,3,0,4,0,2,0x20,0x10,0,0], "H4", 3000)?;

    // Phase 2: x80 channel
    xfer(&h, &[0x0C,0,0,0x28,0x80,0x10,0xED,3,0,0,0,2,0,1,0,0x21,0,0x10,0,0], "X80-H1", 3000)?;
    xfer(&h, &[0x11,0,0,0x18,0x80,0x10,0xED,3,0,2,0,4,0,0x10,0,0,1,0,6,0,1,0,0,0,6,0,0,0], "X80-H2", 3000)?;

    // Phase 3: x2 channel
    xfer(&h, &[0x0C,0,0,0x28,2,0x10,0xF0,3,0,0,0,2,0,1,0,0x21,0,0x10,0,0], "X2-H1", 3000)?;
    xfer(&h, &[0x11,0,0,0x18,2,0x10,0xF0,3,0,2,0,4,0,0x10,0,0,1,0,4,0,1,0,0,0,4,0,0,0], "X2-H2", 3000)?;

    // Phase 4: Open resource 1000 on x80
    xfer(&h, &[
        0x19,0,0,0x18,0x80,0x10,0xED,3,0,3,0,4,
        0x09,0x10,0,0, 1,0,6,0,9,0,0,0,
        0x83,0x66,0xCD,3,0xE8,0x64,0x4C,0x65,0x80,0,0,0
    ], "RSC-1000", 3000)?;

    // Phase 5: Request preset data
    let mut all_data: Vec<u8> = vec![];
    let mut seq: u8 = 4;

    let r = xfer(&h, &[
        0x19,0,0,0x18,0x80,0x10,0xED,3,0,seq,0,0x0C,
        0x0F, 0x10, 0x00, 0,
        1,0,6,0,9,0,0,0,
        0x83,0x66,0xCD,3,0xF4,0x64,0x16,0x65,0xC0,0,0,0
    ], "REQ", 3000)?;

    if r.len() > 16 { all_data.extend_from_slice(&r[16..]); }

    // Phase 6: Read remaining chunks
    for chunk in 1..60 {
        seq = seq.wrapping_add(1);
        let mut buf = [0u8; 4096];
        let _ = h.write_bulk(EP_OUT, &[
            0x08,0,0,0x18,0x80,0x10,0xED,3,0,seq,0,8,
            0x0F, 0x10, 0x00, 0
        ], Duration::from_millis(200));

        match h.read_bulk(EP_IN, &mut buf, Duration::from_millis(2000)) {
            Ok(n) if n > 16 => all_data.extend_from_slice(&buf[16..n]),
            Ok(_) => break,
            Err(rusb::Error::Timeout) => break,
            Err(_) => break,
        }
    }

    h.release_interface(0)?;

    // Output parsed modules
    let data = &all_data;
    let mut pos = 0;
    loop {
        match data[pos..].windows(2).position(|w| w == [0x91, 0x87]) {
            Some(off) => {
                let start = pos + off;
                let rest = &data[start..];
                let mut name_pos = 2;
                while name_pos < rest.len().min(80) {
                    if rest[name_pos] == 0x05 && name_pos + 1 < rest.len() {
                        let nb = rest[name_pos + 1];
                        if nb >= 0xa0 && nb <= 0xbf {
                            let slen = (nb & 0x1f) as usize;
                            if name_pos + 2 + slen <= rest.len() {
                                let raw = &rest[name_pos + 2..name_pos + 2 + slen];
                                let s: String = raw.iter().take_while(|&&c| c != 0).map(|&c| c as char).collect();
                                if !s.is_empty() && s.len() > 1 {
                                    if s.starts_with("SNAPSHOT") {
                                        println!("SNAPSHOT: {s}");
                                    } else {
                                        println!("MODULE: {s}");
                                    }
                                }
                                break;
                            }
                        }
                    }
                    name_pos += 1;
                }
                pos = start + 2;
            }
            None => break,
        }
    }

    // Also output hex for reference
    print!("HEX:");
    for b in &all_data { print!(" {b:02x}"); }
    println!();

    Ok(())
}
