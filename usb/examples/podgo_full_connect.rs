use std::time::Duration;
use anyhow::{anyhow, Result};
use rusb::{Context, UsbContext};

const VID: u16 = 0x0e41;
const PID: u16 = 0x4247;
const EP_OUT: u8 = 0x01;
const EP_IN: u8 = 0x81;
const TIMEOUT: Duration = Duration::from_secs(3);
const DRAIN_TIMEOUT: Duration = Duration::from_millis(50);

fn drain(handle: &rusb::DeviceHandle<Context>) -> Result<()> {
    let mut buf = [0u8; 512];
    loop { match handle.read_bulk(EP_IN, &mut buf, DRAIN_TIMEOUT) { Ok(_) => {}, Err(rusb::Error::Timeout) => break, Err(e) => anyhow::bail!("Drain: {e}") } }
    Ok(())
}

fn wr(handle: &rusb::DeviceHandle<Context>, data: &[u8], label: &str) -> Result<Vec<u8>> {
    let mut buf = [0u8; 4096];
    let _ = handle.write_bulk(EP_OUT, data, TIMEOUT)?;
    let n = handle.read_bulk(EP_IN, &mut buf, TIMEOUT)?;
    let resp = buf[..n].to_vec();
    print!("  {label}: wrote {} read {} ", data.len(), n);
    if n > 7 {
        let is_x1 = buf[4] == 0x01 && buf[5] == 0x10 && buf[6] == 0xEF && buf[7] == 0x03;
        let is_x80 = buf[4] == 0x80 && buf[5] == 0x10 && buf[6] == 0xED && buf[7] == 0x03;
        let is_x2 = buf[4] == 0x02 && buf[5] == 0x10 && buf[6] == 0xF0 && buf[7] == 0x03;
        let hex: Vec<String> = buf[..n.min(48)].iter().map(|b| format!("{b:02x}")).collect();
        print!("  {} bytes: {}", n, hex.join(" "));
        if n > 64 { print!("..."); }
        println!();
        // Also dump full hex for short responses
        if n <= 36 {
            let hex: Vec<String> = resp.iter().map(|b| format!("{b:02x}")).collect();
            println!("    full: {}", hex.join(" "));
        }
    } else {
        println!("  {label}: wrote {} read {}", data.len(), n);
        let hex: Vec<String> = resp.iter().map(|b| format!("{b:02x}")).collect();
        println!("    full: {}", hex.join(" "));
    }
    Ok(resp)
}

fn try_read(handle: &rusb::DeviceHandle<Context>, label: &str) -> Result<Option<Vec<u8>>> {
    let mut buf = [0u8; 4096];
    match handle.read_bulk(EP_IN, &mut buf, Duration::from_millis(200)) {
        Ok(n) => {
            let resp = buf[..n].to_vec();
            let hex: Vec<String> = buf[..n.min(48)].iter().map(|b| format!("{b:02x}")).collect();
            println!("  {label}: read {} bytes: {}", n, hex.join(" "));
            Ok(Some(resp))
        }
        Err(rusb::Error::Timeout) => Ok(None),
        Err(e) => anyhow::bail!("{label}: {e}"),
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
        found.ok_or(anyhow!("Pod Go not found"))?
    };
    handle.set_auto_detach_kernel_driver(true)?;
    handle.claim_interface(0)?;
    handle.clear_halt(EP_OUT)?;
    handle.clear_halt(EP_IN)?;
    drain(&handle)?;

    println!("\n=== Connect-mode handshake + try resources ===");

    // Step 1: HANDSHAKE (same)
    wr(&handle, &[0x0C,0,0,0x28,1,0x10,0xEF,3,0,0,0,2,0,1,0,0x21,0,0x10,0,0], "H1:HANDSHAKE")?;

    // Step 2: SESSION_OPEN_1 with sub-type=5
    wr(&handle, &[0x11,0,0,0x18,1,0x10,0xEF,3,0,2,0,4,0,0x10,0,0,1,0,5,0,1,0,0,0,5,0,0,0], "H2:SESS_OPEN1")?;

    // Step 3: CHUNK_READ offset=0x1020
    wr(&handle, &[0x08,0,0,0x18,1,0x10,0xEF,3,0,3,0,8,0x20,0x10,0,0], "H3:CHUNK_READ")?;

    // Step 4: CMD_0002 (Connect mode uses seq from response = 3)
    wr(&handle, &[0x08,0,0,0x18,1,0x10,0xEF,3,0,3,0,2,0x20,0x10,0,0], "H4:CMD_0002")?;

    // Check if session is fully established
    let resp = handle.read_bulk(EP_IN, &mut [0u8; 512], Duration::from_millis(100));
    match resp {
        Ok(n) => println!("  Extra data after CMD_0002: {n} bytes"),
        Err(rusb::Error::Timeout) => println!("  No extra data"),
        _ => {}
    }

    // Step 5: SESSION_OPEN_2 (same as before but seq=5)
    wr(&handle, &[0x1A,0,0,0x18,1,0x10,0xEF,3,0,5,0,4,9,0x10,0,0,1,0,2,0,0x0A,0,0,0,0x83,0x66,0xCD,3,0xE8,0x64,0xCC,0xFE,0x65,0x80,0,0], "H5:SESS_OPEN2")?;

    // Step 6: CHUNK_READ for session 2
    wr(&handle, &[0x08,0,0,0x18,1,0x10,0xEF,3,0,6,0,8,0x1A,0x10,0,0], "H6:CHUNK_RD2")?;

    println!("\n=== Try resources on x1 (with sub-type=5) ===");

    // Try resource 1003 (preset data?)
    wr(&handle, &[0x19,0,0,0x18,1,0x10,0xEF,3,0,7,0,4,0x1A,0x10,0,0,1,0,2,0,9,0,0,0,0x83,0x66,0xCD,3,0xEB,0x64,0,0x65,0xC0,0,0,0], "RSC:1003")?;

    // Try resource 1004
    wr(&handle, &[0x19,0,0,0x18,1,0x10,0xEF,3,0,8,0,4,0x1A,0x10,0,0,1,0,2,0,9,0,0,0,0x83,0x66,0xCD,3,0xEC,0x64,0,0x65,0xC0,0,0,0], "RSC:1004")?;

    // Try resource 1012
    wr(&handle, &[0x19,0,0,0x18,1,0x10,0xEF,3,0,9,0,4,0x1A,0x10,0,0,1,0,2,0,9,0,0,0,0x83,0x66,0xCD,3,0xF4,0x64,0,0x65,0xC0,0,0,0], "RSC:1012")?;

    // Try resource 1013
    wr(&handle, &[0x19,0,0,0x18,1,0x10,0xEF,3,0,0x0A,0,4,0x1A,0x10,0,0,1,0,2,0,9,0,0,0,0x83,0x66,0xCD,3,0xF5,0x64,0,0x65,0xC0,0,0,0], "RSC:1013")?;

    // Now try opening a stream on 1012 if it succeeded
    println!("\n=== Try 1012 stream (if success) ===");
    wr(&handle, &[0x1D,0,0,0x18,1,0x10,0xEF,3,0,0x0B,0,0x0C,0x38,0x10,0,0,1,0,2,0,0x0D,0,0,0,0x83,0x66,0xCD,3,0xEA,0x64,1,0x65,0x82,0x6B,0,0x65,2,0,0,0], "STR:1012/STREAM")?;

    // Try reading preset data chunks at offset 0x1138
    // (also try resource 1001 to confirm basic session still works)
    println!("\n=== Verify names still work ===");
    wr(&handle, &[0x19,0,0,0x18,1,0x10,0xEF,3,0,0x0C,0,4,0x1A,0x10,0,0,1,0,2,0,9,0,0,0,0x83,0x66,0xCD,3,0xE9,0x64,0,0x65,0xC0,0,0,0], "RSC:1001")?;

    handle.release_interface(0)?;
    println!("\nDone.");
    Ok(())
}
