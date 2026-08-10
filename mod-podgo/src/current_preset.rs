// In-process Pod Go current preset reader
// Uses rusb directly (bypasses the main USB framer/MIDI layer)
// Interface 0 is already released by the device handler for Pod Go

use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
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

/// Run the full connect handshake and read the current preset, returning the
/// raw assembled MessagePack transfer bytes WITHOUT parsing. Used both by
/// `read_current_preset_inprocess` and by RE probes that need un-coerced values
/// (the normal parser turns Integer params into Float).
pub fn read_current_preset_raw() -> Option<Vec<u8>> {
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

    Some(all_data)
}

pub fn read_current_preset_inprocess() -> Option<PresetData> {
    let all_data = read_current_preset_raw()?;

    // Parse modules and snapshots from binary data
    let preset = preset_parser::parse_preset_data(&all_data);
    if preset.modules.is_empty() {
        warn!("No modules found in preset data ({} bytes)", all_data.len());
        return None;
    }

    info!("Read current preset: {} modules, {} footswitches ({} bytes)",
        preset.modules.len(), preset.footswitches.len(), all_data.len());
    store_current_preset_info(preset.clone());
    Some(preset)
}

const EP_OUT: u8 = 0x01;
const EP_IN: u8 = 0x81;

/// A persistent Pod Go read session: runs the connect handshake ONCE, then
/// reads the current preset repeatedly without reconnecting — much faster than
/// `read_current_preset_raw` for polling (used by the RE probe). Releases
/// interface 0 on drop.
pub struct PresetReader {
    handle: rusb::DeviceHandle<rusb::Context>,
}

impl PresetReader {
    pub fn open() -> Option<Self> {
        let handle = podgo_session::find_and_open_podgo().ok()?;
        if let Err(e) = podgo_session::session_init(&handle) {
            warn!("PresetReader session_init failed: {e}");
            let _ = handle.release_interface(0);
            return None;
        }
        Some(PresetReader { handle })
    }

    /// Read + assemble the current preset on the already-open session (no
    /// reconnect). Returns raw MessagePack bytes, or None on a transport hiccup
    /// (the caller can drop and reopen).
    pub fn read_raw(&self) -> Option<Vec<u8>> {
        // Open resource 1000, then request preset data (resource 1012) on x80.
        podgo_session::xfer(&self.handle, &[
            0x19,0,0,0x18,0x80,0x10,0xED,3,0,3,0,4,
            0x09,0x10,0,0, 1,0,6,0,9,0,0,0,
            0x83,0x66,0xCD,3,0xE8,0x64,0x4C,0x65,0x80,0,0,0
        ], 1500).ok()?;
        let r = podgo_session::xfer(&self.handle, &[
            0x19,0,0,0x18,0x80,0x10,0xED,3,0,4,0,0x0C,
            0x0F, 0x10, 0x00, 0,
            1,0,6,0,9,0,0,0,
            0x83,0x66,0xCD,3,0xF4,0x64,0x16,0x65,0xC0,0,0,0
        ], 1500).ok()?;

        let mut all_data: Vec<u8> = vec![];
        if r.len() > 16 { all_data.extend_from_slice(&r[16..]); }

        // Assemble remaining chunks. Shorter terminal timeout than the shared
        // chunks_iter (500ms vs 2000ms) for faster polling; a truncated read
        // just yields a retry on the next poll.
        let mut seq: u8 = 4;
        let mut buf = [0u8; 4096];
        loop {
            seq = seq.wrapping_add(1);
            let _ = self.handle.write_bulk(EP_OUT, &[
                0x08,0,0,0x18,0x80,0x10,0xED,3,0,seq,0,8, 0x0F,0x10,0x00,0
            ], Duration::from_millis(200));
            match self.handle.read_bulk(EP_IN, &mut buf, Duration::from_millis(500)) {
                Ok(n) if n > 16 => all_data.extend_from_slice(&buf[16..n]),
                Ok(_) | Err(rusb::Error::Timeout) => break,
                Err(_) => break,
            }
        }
        if all_data.is_empty() { None } else { Some(all_data) }
    }
}

impl Drop for PresetReader {
    fn drop(&mut self) {
        let _ = self.handle.release_interface(0);
    }
}
