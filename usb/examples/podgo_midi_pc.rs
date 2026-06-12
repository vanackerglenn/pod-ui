// Test: send MIDI PC while session is active on interface 0, see if device sends preset data
use std::time::Duration;
use anyhow::{anyhow, Result};
use rusb::{Context, UsbContext};

const VID: u16 = 0x0e41;
const PID: u16 = 0x4247;
const EP_OUT: u8 = 0x01;
const EP_IN: u8 = 0x81;
const MIDI_OUT: u8 = 0x02;

fn drain(handle: &rusb::DeviceHandle<Context>) {
    let mut buf = [0u8; 512];
    loop { match handle.read_bulk(EP_IN, &mut buf, Duration::from_millis(30)) { Ok(n) => eprintln!("  drain: {n} bytes"), Err(_) => break } }
}

fn xfer(handle: &rusb::DeviceHandle<Context>, data: &[u8], label: &str, ms: u64) -> Result<Vec<u8>> {
    let mut buf = [0u8; 4096];
    handle.write_bulk(EP_OUT, data, Duration::from_millis(ms))?;
    let n = handle.read_bulk(EP_IN, &mut buf, Duration::from_millis(ms))?;
    let resp = buf[..n].to_vec();
    let hex: Vec<String> = resp[..n.min(48)].iter().map(|b| format!("{b:02x}")).collect();
    println!("  {label}: {}b → {}b: {}", data.len(), n, hex.join(" "));
    Ok(resp)
}

fn try_read(handle: &rusb::DeviceHandle<Context>, label: &str, ms: u64) -> Result<Option<Vec<u8>>> {
    let mut buf = [0u8; 4096];
    match handle.read_bulk(EP_IN, &mut buf, Duration::from_millis(ms)) {
        Ok(n) => {
            let hex: Vec<String> = buf[..n.min(48)].iter().map(|b| format!("{b:02x}")).collect();
            println!("  {label}: read {n}b: {}", hex.join(" "));
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

    // Open TWO handles - one for bulk, one for MIDI
    let bulk = dev.open()?;
    bulk.set_auto_detach_kernel_driver(true)?;
    bulk.claim_interface(0)?;
    bulk.clear_halt(EP_OUT)?;
    bulk.clear_halt(EP_IN)?;

    let midi = dev.open()?;
    midi.set_auto_detach_kernel_driver(true)?;
    midi.claim_interface(4)?;
    // MIDI EP 0x02 is OUT (host to device)

    // === Phase 1: Standard handshake on interface 0 ===
    println!("=== Phase 1: x1 handshake ===");
    drain(&bulk);
    xfer(&bulk, &[0x0C,0,0,0x28,1,0x10,0xEF,3,0,0,0,2,0,1,0,0x21,0,0x10,0,0], "HANDSHAKE", 3000)?;
    xfer(&bulk, &[0x11,0,0,0x18,1,0x10,0xEF,3,0,2,0,4,0,0x10,0,0,1,0,2,0,1,0,0,0,2,0,0,0], "SESS_OPEN_1", 3000)?;
    xfer(&bulk, &[0x08,0,0,0x18,1,0x10,0xEF,3,0,3,0,8,0x09,0x10,0,0], "CHUNK_READ", 3000)?;
    xfer(&bulk, &[0x08,0,0,0x18,1,0x10,0xEF,3,0,4,0,2,0x20,0x10,0,0], "CMD_0002", 3000)?;

    // Open sub-session and resource 1001 for names
    xfer(&bulk, &[0x1A,0,0,0x18,1,0x10,0xEF,3,0,5,0,4,0x0A,0x10,0,0,1,0,2,0,0x0A,0,0,0,0x83,0x66,0xCD,3,0xE8,0x64,0xCC,0xFE,0x65,0x80,0,0], "SESS_OPEN_2", 3000)?;
    xfer(&bulk, &[0x08,0,0,0x18,1,0x10,0xEF,3,0,6,0,8,0x1A,0x10,0,0], "CHUNK_RD2", 3000)?;

    // === Phase 2: Send MIDI PC to select preset, then try reading more data ===
    println!("\n=== Phase 2: Send MIDI PC, poll for data ===");

    for preset in [0u8, 1, 2, 5, 10, 20, 50, 100, 127] {
        println!("\n--- Sending PC {preset} ---");

        // Send MIDI PC on separate handle
        let pc = [0xC0, preset];
        midi.write_bulk(MIDI_OUT, &pc, Duration::from_secs(1))?;
        println!("  MIDI: PC {preset} sent");

        // Wait a bit for device to process
        std::thread::sleep(Duration::from_millis(200));

        // Poll interface 0 for any unsolicited data
        for poll in 0..5 {
            if let Some(data) = try_read(&bulk, &format!("poll[{poll}]"), 100)? {
                // Check if this is preset data
                if data.len() > 16 {
                    let hex: Vec<String> = data[..data.len().min(64)].iter().map(|b| format!("{b:02x}")).collect();
                    println!("  => DATA after PC {preset}: {}", hex.join(" "));

                    // Try to send keep-alive/ack
                    match xfer(&bulk, &[0x08,0,0,0x18,1,0x10,0xEF,3,0,0x0F,0,8,0x20,0x1E,0,0], "ACK", 1000) {
                        Ok(r) => println!("  ACK response: {} bytes", r.len()),
                        Err(e) => println!("  ACK: {e}"),
                    }
                }
            } else {
                break; // No more data
            }
        }

        // Also try reading resource 1001 after PC change
        // Maybe the current preset returns different data?
        let rsc_seq = 7 + preset;
        match xfer(&bulk, &[
            0x19,0,0,0x18,1,0x10,0xEF,3,0,rsc_seq,0,4,0x1A,0x10,0,0,
            1,0,2,0,9,0,0,0,
            0x83,0x66,0xCD,3,0xE9,0x64,0,0x65,0xC0,0,0,0
        ], &format!("RSC_1001_after_PC{preset}"), 3000) {
            Ok(r) => {
                if r.len() > 12 {
                    println!("  RSC 1001 status: {:02x}{:02x}", r[12], r[13]);
                }
            }
            Err(e) => println!("  RSC 1001 after PC {preset}: {e}"),
        }
    }

    // === Phase 3: Try to open a stream on the current preset ===
    println!("\n=== Phase 3: Stream current preset ===");
    match xfer(&bulk, &[
        0x1D,0,0,0x18,1,0x10,0xEF,3,0,0x0B,0,0x0C,0x38,0x10,0,0,
        1,0,2,0,0x0D,0,0,0,
        0x83,0x66,0xCD,3,0xEA,0x64,1,0x65,0x82,0x6B,0,0x65,2,0,0,0
    ], "OPEN_STREAM", 3000) {
        Ok(r) => {
            // Read chunks if success
            if r.len() > 12 && r[12] == 0x09 && r[13] == 0x02 {
                println!("  Stream opened! Reading preset names...");
                // The stream offset is at the end of the response
                let off = if r.len() >= 20 {
                    u32::from_le_bytes([r[16], r[17], r[18], r[19]])
                } else { 0x1138 };
                println!("  Stream offset: 0x{off:08x}");

                let mut seq = 0x0Cu8;
                let mut offset = off;
                for chunk in 0..200 {
                    let o = offset.to_le_bytes();
                    match xfer(&bulk, &[0x08,0,0,0x18,1,0x10,0xEF,3,0,seq,0,8, o[0],o[1],o[2],o[3]], &format!("CHUNK[{chunk}]"), 3000) {
                        Ok(data) => {
                            if data.len() < 268 {
                                println!("  End of stream at chunk {chunk}");
                                break;
                            }
                            // Extract preset data from chunk
                            let preset_data = &data[12..]; // Skip header
                            if chunk == 0 {
                                // Dump first few bytes
                                let hex: Vec<String> = preset_data[..24].iter().map(|b| format!("{b:02x}")).collect();
                                println!("  First preset data: {}", hex.join(" "));
                                // Try to find name in the data
                                if let Some(pos) = preset_data.windows(7).position(|w| w[0] > 0x20 && w.iter().all(|&c| c >= 0x20 && c < 0x7f)) {
                                    let run: String = preset_data[pos..].iter().take_while(|&&c| c >= 0x20 && c < 0x7f).map(|&c| c as char).collect();
                                    if run.len() > 2 { println!("  Text found: '{run}'"); }
                                }
                            }
                        }
                        Err(e) => { println!("  Chunk {chunk}: {e}"); break; }
                    }
                    offset += 0x100;
                    seq = seq.wrapping_add(1);
                }
            } else {
                println!("  Stream open failed or no data");
            }
        }
        Err(e) => println!("  Stream: {e}"),
    }

    bulk.release_interface(0)?;
    midi.release_interface(4)?;
    println!("\nDone.");
    Ok(())
}
