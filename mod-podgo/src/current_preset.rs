// In-process Pod Go current preset reader
// Uses rusb directly (bypasses the main USB framer/MIDI layer)
// Interface 0 is already released by the device handler for Pod Go

use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use log::*;

use crate::preset_parser::{self, PresetData};
use crate::podgo_session;

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

pub fn read_current_preset_inprocess() -> Option<PresetData> {
    // Find and open POD Go device, then perform channel setup handshake
    let handle = match podgo_session::find_and_open_podgo() {
        Ok(h) => h,
        Err(e) => {
            warn!("Failed to find/open POD Go: {e}");
            return None;
        }
    };

    if let Err(e) = podgo_session::session_init(&handle) {
        warn!("Failed to initialize session: {e}");
        let _ = handle.release_interface(0);
        return None;
    }

    // Phase 4: Open resource 1000 on x80
    if let Err(e) = podgo_session::xfer(&handle, &[
        0x19,0,0,0x18,0x80,0x10,0xED,3,0,3,0,4,
        0x09,0x10,0,0, 1,0,6,0,9,0,0,0,
        0x83,0x66,0xCD,3,0xE8,0x64,0x4C,0x65,0x80,0,0,0
    ], 3000) {
        warn!("Open resource 1000 failed: {e}");
        let _ = handle.release_interface(0);
        return None;
    }

    // Phase 5: Request preset data
    let r = match podgo_session::xfer(&handle, &[
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
    for chunk in podgo_session::chunks_iter(&handle) { all_data.extend_from_slice(&chunk); }

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
