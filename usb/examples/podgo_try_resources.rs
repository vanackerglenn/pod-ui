use std::time::Duration;
use anyhow::{anyhow, Result};
use rusb::{Context, UsbContext};

const VID: u16 = 0x0e41;
const PID: u16 = 0x4247;
const EP_OUT: u8 = 0x01;
const EP_IN: u8 = 0x81;
const TIMEOUT: Duration = Duration::from_secs(2);
const DRAIN_TIMEOUT: Duration = Duration::from_millis(50);
const MAX_CHUNK: usize = 272;

fn drain(handle: &rusb::DeviceHandle<Context>) -> Result<()> {
    let mut buf = [0u8; 512];
    loop { match handle.read_bulk(EP_IN, &mut buf, DRAIN_TIMEOUT) { Ok(_) => {}, Err(rusb::Error::Timeout) => break, Err(e) => anyhow::bail!("Drain: {e}") } }
    Ok(())
}

fn xfer(handle: &rusb::DeviceHandle<Context>, data: &[u8], label: &str) -> Result<usize> {
    let mut buf = [0u8; 512];
    let _ = handle.write_bulk(EP_OUT, data, TIMEOUT)?;
    let n = handle.read_bulk(EP_IN, &mut buf, TIMEOUT)?;
    if n > 12 {
        println!("  {label}: wrote {} read {} (payload below)", data.len(), n);
        let show = &buf[12..n.min(92)];
        for chunk in show.chunks(16) {
            let hex: Vec<String> = chunk.iter().map(|b| format!("{b:02x}")).collect();
            println!("    {}", hex.join(" "));
        }
    } else {
        println!("  {label}: wrote {} read {} (no payload)", data.len(), n);
    }
    Ok(n)
}

fn msgpack_open_resource(resource_id: u16) -> Vec<u8> {
    let mut p = vec![0x83, 0x66]; // fixmap(3), key=102
    if resource_id <= 0xFF {
        p.extend_from_slice(&[0xCC, resource_id as u8]); // uint8
    } else {
        p.extend_from_slice(&[0xCD, (resource_id >> 8) as u8, resource_id as u8]); // uint16
    }
    p.extend_from_slice(&[0x64, 0x00, 0x65, 0xC0]); // key=100:0, key=101:nil
    p
}

fn make_open_packet(resource_id: u16, seq: u8) -> Vec<u8> {
    let payload = msgpack_open_resource(resource_id);
    let total_len: u8 = 24 + payload.len() as u8;
    let mut pkt = vec![
        total_len, 0, 0, 0x18,
        0x01, 0x10, 0xEF, 0x03,
        0, seq,
        0, 0x04, // cmd=open resource
        0x1A, 0x10, 0, 0,
        1, 0, 2, 0,
    ];
    let plen = payload.len() as u32;
    pkt.extend_from_slice(&plen.to_le_bytes());
    pkt.extend_from_slice(&payload);
    while pkt.len() % 4 != 0 { pkt.push(0); }
    pkt
}

fn make_stream_packet(seq: u8) -> Vec<u8> {
    let payload: [u8;13] = [0x83,0x66,0xCD,3,0xEA,0x64,1,0x65,0x82,0x6B,0,0x65,2];
    let mut pkt = vec![
        0x1D, 0, 0, 0x18,
        0x01, 0x10, 0xEF, 0x03,
        0, seq,
        0, 0x0C,
        0x38, 0x10, 0, 0,
        1, 0, 2, 0,
    ];
    let plen = 13u32;
    pkt.extend_from_slice(&plen.to_le_bytes());
    pkt.extend_from_slice(&payload);
    pkt
}

fn make_chunk(seq: u8, offset: u32) -> [u8; 16] {
    let o = offset.to_le_bytes();
    [0x08,0,0,0x18,1,0x10,0xEF,3,0,seq,0,0x08,o[0],o[1],o[2],o[3]]
}

fn read_resource(handle: &rusb::DeviceHandle<Context>, resource_id: u16, label: &str) -> Result<Vec<u8>> {
    let mut buf = [0u8; 512];
    let open = make_open_packet(resource_id, 6);
    xfer(handle, &open, &format!("{label} open"))?;

    let stream = make_stream_packet(7);
    let n = xfer(handle, &stream, &format!("{label} stream"))?;
    let mut data = Vec::new();
    if n > 12 { data.extend_from_slice(&buf[12..n]); }

    let mut seq: u8 = 8;
    let mut offset: u32 = 0x1138;
    loop {
        let req = make_chunk(seq, offset);
        let n = xfer(handle, &req, &format!("{label} chunk[{seq}]"))?;
        if n > 12 { data.extend_from_slice(&buf[12..n]); }
        if n < MAX_CHUNK { break; }
        offset += 0x0100;
        seq += 1;
    }
    println!("  {label}: total {} bytes", data.len());
    Ok(data)
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

    // Handshake
    let h1:[u8;20]=[0x0C,0,0,0x28,1,0x10,0xEF,3,0,0,0,2,0,1,0,0x21,0,0x10,0,0];
    let s1:[u8;28]=[0x11,0,0,0x18,1,0x10,0xEF,3,0,2,0,4,0,0x10,0,0,1,0,2,0,1,0,0,0,2,0,0,0];
    let c1:[u8;16]=[0x08,0,0,0x18,1,0x10,0xEF,3,0,3,0,8,9,0x10,0,0];
    let s2:[u8;36]=[0x1A,0,0,0x18,1,0x10,0xEF,3,0,4,0,4,9,0x10,0,0,1,0,2,0,0x0A,0,0,0,0x83,0x66,0xCD,3,0xE8,0x64,0xCC,0xFE,0x65,0x80,0,0];
    let c2:[u8;16]=[0x08,0,0,0x18,1,0x10,0xEF,3,0,5,0,8,0x1A,0x10,0,0];
    xfer(&handle, &h1, "HANDSHAKE")?;
    xfer(&handle, &s1, "SESSION_OPEN_1")?;
    xfer(&handle, &c1, "SESSION_CHUNK_1")?;
    xfer(&handle, &s2, "SESSION_OPEN_2")?;
    xfer(&handle, &c2, "SESSION_CHUNK_2")?;

    // Try different resources on x1 channel
    let resources: [u16; 6] = [1003, 1004, 1005, 1006, 1012, 1013];
    for (i, rid) in resources.iter().enumerate() {
        let label = format!("Resource {rid}");
        match read_resource(&handle, *rid, &label) {
            Ok(data) if !data.is_empty() => {
                // Search for interesting markers
                if data.contains(&0xDC) || data.len() > 100 {
                    println!("  => POTENTIALLY INTERESTING: {} bytes, contains array marker", data.len());
                    // Show hex preview
                    for chunk in data[..data.len().min(128)].chunks(16) {
                        let hex: Vec<String> = chunk.iter().map(|b| format!("{b:02x}")).collect();
                        println!("    {}", hex.join(" "));
                    }
                }
            }
            Ok(_) => {}
            Err(e) => println!("  {label}: error: {e}"),
        }

        // Re-drain after each attempt
        let _ = drain(&handle);
        // Re-send handshake for a clean state
        xfer(&handle, &h1, &format!("re-handshake[{i}]"))?;
        xfer(&handle, &s1, &format!("re-s1[{i}]"))?;
        xfer(&handle, &c1, &format!("re-c1[{i}]"))?;
        xfer(&handle, &s2, &format!("re-s2[{i}]"))?;
        xfer(&handle, &c2, &format!("re-c2[{i}]"))?;
    }

    handle.release_interface(0)?;
    Ok(())
}
