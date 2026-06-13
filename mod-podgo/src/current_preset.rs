// In-process Pod Go current preset reader
// Uses rusb directly (bypasses the main USB framer/MIDI layer)
// Interface 0 is already released by the device handler for Pod Go

use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use anyhow::Result;
use log::*;
use rusb::{Context, UsbContext};

use crate::preset_parser::{self, PresetData};

static CURRENT_PRESET: Mutex<Option<PresetData>> = Mutex::new(None);
static PRESET_VERSION: AtomicU64 = AtomicU64::new(0);

pub fn store_current_preset_info(preset: PresetData) {
    if let Ok(mut p) = CURRENT_PRESET.lock() {
        *p = Some(preset);
        PRESET_VERSION.fetch_add(1, Ordering::SeqCst);
    }
}

pub fn get_current_preset_info() -> Option<PresetData> {
    CURRENT_PRESET.lock().ok().and_then(|p| p.clone())
}

pub fn get_preset_version() -> u64 {
    PRESET_VERSION.load(Ordering::SeqCst)
}

const VID: u16 = 0x0e41;
const PID: u16 = 0x4247;
const EP_OUT: u8 = 0x01;
const EP_IN: u8 = 0x81;

fn drain(handle: &rusb::DeviceHandle<rusb::Context>) {
    let mut buf = [0u8; 512];
    loop { match handle.read_bulk(EP_IN, &mut buf, Duration::from_millis(30)) { Ok(_) => {}, Err(_) => break } }
}

fn xfer(handle: &rusb::DeviceHandle<Context>, data: &[u8], ms: u64) -> Result<Vec<u8>> {
    let mut buf = [0u8; 4096];
    handle.write_bulk(EP_OUT, data, Duration::from_millis(ms))?;
    let n = handle.read_bulk(EP_IN, &mut buf, Duration::from_millis(ms))?;
    Ok(buf[..n].to_vec())
}

fn chunks_iter(handle: &rusb::DeviceHandle<Context>) -> Vec<Vec<u8>> {
    let mut chunks = vec![];
    let mut seq: u8 = 4;
    loop {
        seq = seq.wrapping_add(1);
        let mut buf = [0u8; 4096];
        let _ = handle.write_bulk(EP_OUT, &[
            0x08,0,0,0x18,0x80,0x10,0xED,3,0,seq,0,8,
            0x0F, 0x10, 0x00, 0
        ], Duration::from_millis(200));

        match handle.read_bulk(EP_IN, &mut buf, Duration::from_millis(2000)) {
            Ok(n) if n > 16 => { chunks.push(buf[16..n].to_vec()); }
            Ok(_) | Err(rusb::Error::Timeout) => break,
            Err(_) => break,
        }
    }
    chunks
}

pub fn read_current_preset_inprocess() -> Option<PresetData> {
    let ctx = match Context::new() {
        Ok(c) => c,
        Err(e) => { warn!("Failed to create USB context: {e}"); return None; }
    };

    let devices = match ctx.devices() {
        Ok(d) => d,
        Err(e) => { warn!("Failed to list USB devices: {e}"); return None; }
    };

    let dev = match devices.iter().find(|d| {
        d.device_descriptor().map(|dd| dd.vendor_id() == VID && dd.product_id() == PID).unwrap_or(false)
    }) {
        Some(d) => d,
        None => { warn!("Pod Go not found"); return None; }
    };

    let handle = match dev.open() {
        Ok(h) => h,
        Err(e) => { warn!("Failed to open Pod Go: {e}"); return None; }
    };

    if let Err(e) = handle.set_auto_detach_kernel_driver(true) {
        warn!("set_auto_detach_kernel_driver: {e}");
    }

    if let Err(e) = handle.claim_interface(0) {
        warn!("Failed to claim interface 0: {e}");
        return None;
    }
    let _ = handle.clear_halt(EP_OUT);
    let _ = handle.clear_halt(EP_IN);
    drain(&handle);

    // Phase 1: x1 session (sub_type=5)
    if let Err(e) = xfer(&handle, &[0x0C,0,0,0x28,1,0x10,0xEF,3,0,0,0,2,0,1,0,0x21,0,0x10,0,0], 3000)
        .and_then(|_| xfer(&handle, &[0x11,0,0,0x18,1,0x10,0xEF,3,0,2,0,4,0,0x10,0,0,1,0,5,0,1,0,0,0,5,0,0,0], 3000))
        .and_then(|_| xfer(&handle, &[0x08,0,0,0x18,1,0x10,0xEF,3,0,3,0,8,0x20,0x10,0,0], 3000))
        .and_then(|_| xfer(&handle, &[0x08,0,0,0x18,1,0x10,0xEF,3,0,4,0,2,0x20,0x10,0,0], 3000))
    {
        warn!("x1 session failed: {e}");
        let _ = handle.release_interface(0);
        return None;
    }

    // Phase 2: x80 channel
    if let Err(e) = xfer(&handle, &[0x0C,0,0,0x28,0x80,0x10,0xED,3,0,0,0,2,0,1,0,0x21,0,0x10,0,0], 3000)
        .and_then(|_| xfer(&handle, &[0x11,0,0,0x18,0x80,0x10,0xED,3,0,2,0,4,0,0x10,0,0,1,0,6,0,1,0,0,0,6,0,0,0], 3000))
    {
        warn!("x80 channel failed: {e}");
        let _ = handle.release_interface(0);
        return None;
    }

    // Phase 3: x2 channel
    if let Err(e) = xfer(&handle, &[0x0C,0,0,0x28,2,0x10,0xF0,3,0,0,0,2,0,1,0,0x21,0,0x10,0,0], 3000)
        .and_then(|_| xfer(&handle, &[0x11,0,0,0x18,2,0x10,0xF0,3,0,2,0,4,0,0x10,0,0,1,0,4,0,1,0,0,0,4,0,0,0], 3000))
    {
        warn!("x2 channel failed: {e}");
        let _ = handle.release_interface(0);
        return None;
    }

    // Phase 4: Open resource 1000 on x80
    if let Err(e) = xfer(&handle, &[
        0x19,0,0,0x18,0x80,0x10,0xED,3,0,3,0,4,
        0x09,0x10,0,0, 1,0,6,0,9,0,0,0,
        0x83,0x66,0xCD,3,0xE8,0x64,0x4C,0x65,0x80,0,0,0
    ], 3000) {
        warn!("Open resource 1000 failed: {e}");
        let _ = handle.release_interface(0);
        return None;
    }

    // Phase 5: Request preset data
    let r = match xfer(&handle, &[
        0x19,0,0,0x18,0x80,0x10,0xED,3,0,4,0,0x0C,
        0x0F, 0x10, 0x00, 0,
        1,0,6,0,9,0,0,0,
        0x83,0x66,0xCD,3,0xF4,0x64,0x16,0x65,0xC0,0,0,0
    ], 3000) {
        Ok(r) => r,
        Err(e) => {
            warn!("Request preset data failed: {e}");
            let _ = handle.release_interface(0);
            return None;
        }
    };

    // Phase 6: Assemble all data chunks
    let mut all_data: Vec<u8> = vec![];
    if r.len() > 16 { all_data.extend_from_slice(&r[16..]); }
    for chunk in chunks_iter(&handle) { all_data.extend_from_slice(&chunk); }

    let _ = handle.release_interface(0);

    // Optional diagnostic: set PODGO_DUMP=1 to write the raw preset bytes for
    // offline MessagePack analysis (used while reverse-engineering the format).
    if std::env::var("PODGO_DUMP").is_ok() {
        let path = std::env::temp_dir().join("podgo_preset.bin");
        match std::fs::write(&path, &all_data) {
            Ok(_) => info!("PODGO_DUMP: wrote {} raw preset bytes to {}", all_data.len(), path.display()),
            Err(e) => warn!("PODGO_DUMP: failed to write raw preset dump: {e}"),
        }
    }

    // Parse modules and snapshots from binary data
    let preset = preset_parser::parse_preset_data(&all_data);
    if preset.modules.is_empty() {
        warn!("No modules found in preset data ({} bytes)", all_data.len());
        let _ = handle.release_interface(0);
        return None;
    }

    info!("Read current preset: {} modules, {} footswitches ({} bytes)",
        preset.modules.len(), preset.footswitches.len(), all_data.len());
    store_current_preset_info(preset.clone());
    Some(preset)
}
