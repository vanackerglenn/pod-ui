//! POD Go persistent USB session and edit-buffer worker
//!
//! # Design: persistent session + command queue + polling event loop
//!
//! **Context:** Currently, pod-ui reads the POD Go device's edit buffer with a one-shot
//! open-claim-release-close cycle (in `mod-podgo::current_preset::read_current_preset_inprocess`).
//! This is fragile (device state lost between calls) and doesn't support live edits (writes).
//!
//! **Decision:** Replace the per-call contention with **one persistent `rusb` device handle**,
//! owned by a **dedicated background worker thread**. The session does three jobs:
//!
//! 1. **Channel-open handshake once**: When the session starts, execute Phases 1–3
//!    (x1/x80/x2 channel setup) from the existing `read_current_preset_inprocess` inline code.
//!    This establishes the three logical channels (setlist x1, edit-buffer x80, notifications x2).
//!
//! 2. **Service a command queue**: GTK edit callbacks (param sliders, bypass checkboxes, model combos)
//!    enqueue non-blocking write commands. The worker drains the queue and sends them to the device
//!    via the x80 channel, handling the request/ack envelope, maintaining a monotonic txn counter,
//!    and surfacing errors (retry, log, skip).
//!
//! 3. **Poll x2 for device-origin events**: The worker continuously polls the x2 channel for
//!    notifications (the device pushes when its knobs/buttons are turned). Each event is decoded,
//!    type-checked, and forwarded to the GTK side on an event channel. The GTK layer applies
//!    them back to the Controller with `StoreOrigin::Device` to prevent feedback loops.
//!
//! **Benefits:**
//! - One owner of interface 0 — no arbitration between reads and writes.
//! - Non-blocking from the GTK side (enqueue, fire-and-forget).
//! - Live edit buffer: writes are persistent until the next device read (or a device preset load).
//! - Device-initiated edits: x2 polling allows live knob/button feedback without UI refresh hammering.
//! - Testable: the worker's I/O can be unit-tested offline by mocking the USB packets.
//!
//! # Public surface (to be implemented)
//!
//! ## Enqueue API (GTK → worker thread)
//!
//! ```ignore
//! pub fn enqueue_write(cmd: WriteCmd) -> Result<()>;
//! ```
//!
//! Where `WriteCmd` is an enum of:
//! - `SetBypass { slot: u8, enabled: bool }` — op 41
//! - `SetParam { slot: u8, param_idx: u8, value: ParamValue }` — op 30
//!   - `ParamValue`: carries the wire type (f32 0..1 for continuous, u32 index for enum)
//! - `SetModel { slot: u8, model_id: u32 }` — op = model id, with focus first
//! - `ReadPreset { on_complete: Callback }` — full preset read via x80 (for device preset load, model change follow-up)
//!
//! Returns `Ok(())` if enqueued; `Err(_)` if the queue is full or the worker is dead.
//! Does NOT wait for the device to ack.
//!
//! ## Event channel (worker → GTK)
//!
//! ```ignore
//! pub fn event_rx() -> mpsc::Receiver<SessionEvent>;
//! ```
//!
//! Yields events:
//! - `SessionEvent::X2Notification { slot: u8, param_idx: u8, value: ParamValue }`
//!   — a device-originated param edit (device knob/button turned).
//! - `SessionEvent::PresetLoaded(PresetData)` — a full preset read completed (in response to
//!   a `ReadPreset` enqueue or a device preset change detected).
//! - `SessionEvent::Error(String)` — worker encountered a fatal error (e.g. device unplugged).
//!   GTK should fall back to one-shot read-only mode.
//!
//! ## Lifecycle
//!
//! ```ignore
//! pub fn start() -> Result<()>;  // spawn the worker thread (once per app)
//! pub fn shutdown();             // signal the worker to exit cleanly (called on app close)
//! ```
//!
//! The worker remains alive until `shutdown()` or a fatal device error. `start()` is idempotent
//! (or panics if already running, depending on design).
//!
//! # Implementation plan (Phases 1.2–1.4)
//!
//! - **Phase 1.2**: Extract the x1/x80/x2 channel-open handshake into a reusable `session_init()`.
//!   Keep `xfer`/`drain`/`chunks_iter` helpers (shared by all channel reads).
//! - **Phase 1.3**: Implement the write-command encoder (ops 30/41/78 per `usb/docs/podgo-write-protocol.md`).
//!   Add offline unit tests (byte-match against `mod-podgo/captures/02,03,05.txt`).
//! - **Phase 1.4**: Implement the kind-aware value mapping (enum/bool/float conversions) in `mod-podgo/src/model.rs`.
//! - **Phase 2+**: Plumb GTK callbacks to `enqueue_write()` and apply x2 events back to the Controller.
//!
//! # Notes
//!
//! - The current `read_current_preset_inprocess()` will be refactored to use this session's
//!   preset-read API (Phase 1.2). Existing callers continue to work (blocking, one-shot read).
//! - The session is optional for the app (Pod Go presence is detected at init time).
//!   If the device is absent, the session thread doesn't start; GTK falls back gracefully.
//! - Feedback-loop safety is the GTK layer's job (via `StoreOrigin` / `updating` flag).
//!   The worker doesn't filter anything — it encodes, sends, and forwards events.

use std::time::Duration;
use anyhow::Result;
use log::*;
use rusb::{Context, UsbContext};

const VID: u16 = 0x0e41;
const PID: u16 = 0x4247;
const EP_OUT: u8 = 0x01;
const EP_IN: u8 = 0x81;

/// Helper: drain any pending data from the device (clears stale packets)
pub fn drain(handle: &rusb::DeviceHandle<rusb::Context>) {
    let mut buf = [0u8; 512];
    loop {
        match handle.read_bulk(EP_IN, &mut buf, Duration::from_millis(30)) {
            Ok(_) => {},
            Err(_) => break
        }
    }
}

/// Helper: send request, wait for response in a single xfer
pub fn xfer(handle: &rusb::DeviceHandle<Context>, data: &[u8], ms: u64) -> Result<Vec<u8>> {
    let mut buf = [0u8; 4096];
    handle.write_bulk(EP_OUT, data, Duration::from_millis(ms))?;
    let n = handle.read_bulk(EP_IN, &mut buf, Duration::from_millis(ms))?;
    Ok(buf[..n].to_vec())
}

/// Helper: read chunked preset data (used by preset read; each chunk is 272 bytes, strips the 16-byte header)
pub fn chunks_iter(handle: &rusb::DeviceHandle<Context>) -> Vec<Vec<u8>> {
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

/// Find and open the POD Go device, claim interface 0
pub fn find_and_open_podgo() -> Result<rusb::DeviceHandle<rusb::Context>> {
    let ctx = Context::new()?;
    let devices = ctx.devices()?;

    let dev = devices.iter().find(|d| {
        d.device_descriptor()
            .map(|dd| dd.vendor_id() == VID && dd.product_id() == PID)
            .unwrap_or(false)
    }).ok_or_else(|| anyhow::anyhow!("POD Go device not found"))?;

    let handle = dev.open()?;

    if let Err(e) = handle.set_auto_detach_kernel_driver(true) {
        warn!("set_auto_detach_kernel_driver: {e}");
    }

    handle.claim_interface(0)?;
    let _ = handle.clear_halt(EP_OUT);
    let _ = handle.clear_halt(EP_IN);

    Ok(handle)
}

/// Initialize the three USB channels (x1, x80, x2) via the handshake sequence.
/// This is the one-time setup; after this, the session can send writes and poll for events.
pub fn session_init(handle: &rusb::DeviceHandle<Context>) -> Result<()> {
    drain(handle);

    // Phase 1: x1 session (setlist resource)
    xfer(handle, &[0x0C,0,0,0x28,1,0x10,0xEF,3,0,0,0,2,0,1,0,0x21,0,0x10,0,0], 3000)
        .and_then(|_| xfer(handle, &[0x11,0,0,0x18,1,0x10,0xEF,3,0,2,0,4,0,0x10,0,0,1,0,5,0,1,0,0,0,5,0,0,0], 3000))
        .and_then(|_| xfer(handle, &[0x08,0,0,0x18,1,0x10,0xEF,3,0,3,0,8,0x20,0x10,0,0], 3000))
        .and_then(|_| xfer(handle, &[0x08,0,0,0x18,1,0x10,0xEF,3,0,4,0,2,0x20,0x10,0,0], 3000))
        .map_err(|e| {
            warn!("x1 session failed: {e}");
            e
        })?;

    // Phase 2: x80 channel (edit buffer / write commands)
    xfer(handle, &[0x0C,0,0,0x28,0x80,0x10,0xED,3,0,0,0,2,0,1,0,0x21,0,0x10,0,0], 3000)
        .and_then(|_| xfer(handle, &[0x11,0,0,0x18,0x80,0x10,0xED,3,0,2,0,4,0,0x10,0,0,1,0,6,0,1,0,0,0,6,0,0,0], 3000))
        .map_err(|e| {
            warn!("x80 channel failed: {e}");
            e
        })?;

    // Phase 3: x2 channel (notifications / device events)
    xfer(handle, &[0x0C,0,0,0x28,2,0x10,0xF0,3,0,0,0,2,0,1,0,0x21,0,0x10,0,0], 3000)
        .and_then(|_| xfer(handle, &[0x11,0,0,0x18,2,0x10,0xF0,3,0,2,0,4,0,0x10,0,0,1,0,4,0,1,0,0,0,4,0,0,0], 3000))
        .map_err(|e| {
            warn!("x2 channel failed: {e}");
            e
        })?;

    Ok(())
}
