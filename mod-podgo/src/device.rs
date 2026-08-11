//! The connection to the device: opened once, kept open.
//!
//! USB interface 0 has exactly one owner, so everything that talks to the POD
//! Go has to go through one place. This is that place. It is opened when the
//! device is detected and held for as long as the program runs — reading a
//! patch no longer costs a connect, a handshake and a disconnect.
//!
//! **Phase 1 does reading only.** Writing edits and receiving the device's own
//! changes come later and are deliberately absent; see
//! `usb/docs/podgo-architecture.md`.
//!
//! # What reusing a connection changes
//!
//! The byte sequences here are the ones that have always worked, with two
//! changes, both forced by the connection outliving a single operation:
//!
//! * **Sequence numbers must keep climbing.** Byte 9 of every frame is a
//!   per-channel counter, and the device *silently discards* a frame whose
//!   number it has already seen — no reply, no error. Every operation used to
//!   open its own connection, so hardcoded numbers starting at 3 were fine;
//!   reused, the second read replays them and simply hangs.
//!
//! * **The next frame in is not necessarily the answer.** The connection
//!   multiplexes three channels and the device emits a 16-byte heartbeat about
//!   once a second, so a read must skip what isn't its own. Taking the next
//!   frame regardless truncates a patch at whatever page the heartbeat lands
//!   between, or returns nothing at all.
//!
//! Everything else about the frames is untouched, including the value in bytes
//! 12..16 — that is a flow-control counter which matters for sustained
//! traffic, and Phase 1 has none.
//!
//! # Falling back
//!
//! Loading is what already worked, and it is never allowed to depend on this.
//! If a read over the open connection fails twice, the connection is
//! **released first** and the read retried on the original one-shot path.
//! Releasing first is not optional: that path opens its own connection, and it
//! cannot while this one is held — its handshake would just time out.

use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use log::*;

use crate::podgo_session;
use crate::preset_parser::{self, PresetData};

const EP_OUT: u8 = 0x01;
const EP_IN: u8 = 0x81;

/// A full stream page. A shorter one is the last.
const PAGE: usize = 272;

/// The x80 channel, as the four bytes at 4..8 of an inbound frame. Outbound
/// frames carry the same four reversed, which is easy to get wrong.
const X80_IN: [u8; 4] = [0xED, 0x03, 0x80, 0x10];

/// Open the preset resource. Verbatim from the read path that works; only
/// byte 9, the sequence number, is filled in per use.
const OPEN_RESOURCE: [u8; 36] = [
    0x19,0,0,0x18, 0x80,0x10,0xED,3, 0,0, 0,4,
    0x09,0x10,0,0, 1,0,6,0, 9,0,0,0,
    0x83,0x66,0xCD,3,0xE8,0x64,0x4C,0x65,0x80,0,0,0,
];

/// Ask for the current preset. The reply is the head of a paged stream.
const READ_PRESET: [u8; 36] = [
    0x19,0,0,0x18, 0x80,0x10,0xED,3, 0,0, 0,0x0C,
    0x0F,0x10,0x00,0, 1,0,6,0, 9,0,0,0,
    0x83,0x66,0xCD,3,0xF4,0x64,0x16,0x65,0xC0,0,0,0,
];

/// Ask for the next page.
const PULL_PAGE: [u8; 16] = [
    0x08,0,0,0x18, 0x80,0x10,0xED,3, 0,0, 0,8, 0x0F,0x10,0x00,0,
];

pub struct Device {
    handle: rusb::DeviceHandle<rusb::Context>,
    /// Monotonic for the life of the connection. The handshake uses 0 and 2 on
    /// x80, so the first frame after it is 3.
    seq_x80: AtomicU8,
}

impl Drop for Device {
    fn drop(&mut self) {
        let _ = self.handle.release_interface(0);
        debug!("Pod Go: connection closed");
    }
}

impl Device {
    /// Claim the interface and run the channel handshake.
    pub fn open() -> Option<Device> {
        let handle = match podgo_session::find_and_open_podgo() {
            Ok(h) => h,
            Err(e) => {
                warn!("Pod Go: cannot open the device: {e}");
                return None;
            }
        };
        if let Err(e) = podgo_session::session_init(&handle) {
            warn!("Pod Go: handshake failed: {e}");
            let _ = handle.release_interface(0);
            return None;
        }
        Some(Device { handle, seq_x80: AtomicU8::new(3) })
    }

    fn next_seq(&self) -> u8 {
        self.seq_x80.fetch_add(1, Ordering::SeqCst)
    }

    fn send(&self, frame: &[u8]) -> bool {
        self.handle.write_bulk(EP_OUT, frame, Duration::from_millis(500)).is_ok()
    }

    /// The next frame on x80 that carries something, ignoring other channels
    /// and the idle heartbeat.
    fn recv(&self, timeout: Duration) -> Option<Vec<u8>> {
        let deadline = Instant::now() + timeout;
        let mut buf = [0u8; 4096];
        loop {
            let left = deadline.checked_duration_since(Instant::now())?;
            let n = self.handle.read_bulk(EP_IN, &mut buf, left).ok()?;
            if n < 16 || buf[4..8] != X80_IN {
                continue; // another channel's traffic
            }
            if n == 16 {
                continue; // the idle heartbeat carries nothing
            }
            return Some(buf[..n].to_vec());
        }
    }

    /// Read the current preset, returning the raw assembled bytes.
    pub fn read_preset_raw(&self) -> Option<Vec<u8>> {
        let mut open = OPEN_RESOURCE;
        open[9] = self.next_seq();
        if !self.send(&open) {
            return None;
        }
        self.recv(Duration::from_millis(1500))?;

        let mut request = READ_PRESET;
        request[9] = self.next_seq();
        if !self.send(&request) {
            return None;
        }

        // The reply to the request is the head of the stream; every page after
        // it is asked for. A page shorter than a full one is the last.
        let mut data: Vec<u8> = vec![];
        let mut page = self.recv(Duration::from_millis(1500))?;
        loop {
            data.extend_from_slice(&page[16..]);
            if page.len() < PAGE {
                break;
            }
            let mut pull = PULL_PAGE;
            pull[9] = self.next_seq();
            if !self.send(&pull) {
                break;
            }
            match self.recv(Duration::from_millis(700)) {
                Some(next) => page = next,
                None => break,
            }
        }

        (!data.is_empty()).then_some(data)
    }
}

/// The one connection, for as long as the program runs.
static DEVICE: Mutex<Option<Device>> = Mutex::new(None);

fn held() -> std::sync::MutexGuard<'static, Option<Device>> {
    // A poisoned lock must not disable the device for the rest of the run.
    DEVICE.lock().unwrap_or_else(|e| e.into_inner())
}

/// Open the connection. Safe to call again; does nothing if one is already up.
///
/// Retried, for the same reason reading retries: claiming interface 0 straight
/// after the preset-name fetch has released it loses a race often enough to
/// matter.
pub fn connect() -> bool {
    if held().is_some() {
        return true;
    }
    if std::env::var("PODGO_ONE_SHOT").is_ok_and(|v| v != "0") {
        info!("Pod Go: PODGO_ONE_SHOT set, every read will open its own connection");
        return false;
    }

    const ATTEMPTS: u32 = 4;
    for attempt in 1..=ATTEMPTS {
        if let Some(dev) = Device::open() {
            *held() = Some(dev);
            info!("Pod Go: connection open");
            return true;
        }
        if attempt < ATTEMPTS {
            std::thread::sleep(Duration::from_millis(300 * attempt as u64));
        }
    }
    warn!(
        "Pod Go: could not keep a connection open; \
         each read will open its own, as before"
    );
    false
}

pub fn is_connected() -> bool {
    held().is_some()
}

/// Close the connection and release the interface.
pub fn disconnect() {
    if held().take().is_some() {
        debug!("Pod Go: connection released");
    }
}

/// Read the current preset and cache it.
///
/// Uses the open connection when there is one, and falls back to a connection
/// of its own if that fails — releasing the open one first, since the device
/// admits only one.
pub fn read_preset() -> Option<PresetData> {
    let over_connection = {
        let guard = held();
        match guard.as_ref() {
            Some(dev) => {
                // One failure is not reason enough to give up a working
                // connection; the device may simply have been busy.
                dev.read_preset_raw().or_else(|| {
                    debug!("Pod Go: the preset did not read, retrying");
                    dev.read_preset_raw()
                })
            }
            None => None,
        }
    };

    if let Some(data) = over_connection {
        return parse_and_store(&data);
    }

    if is_connected() {
        warn!("Pod Go: reading over the open connection failed, falling back");
        // Release before anything else claims the interface.
        disconnect();
    }
    crate::current_preset::read_current_preset_inprocess()
}

fn parse_and_store(data: &[u8]) -> Option<PresetData> {
    let preset = preset_parser::parse_preset_data(data);
    if preset.modules.is_empty() {
        warn!("Pod Go: the preset parsed no modules ({} bytes)", data.len());
        return None;
    }
    info!(
        "Pod Go: read preset ({} modules, {} footswitches, {} bytes)",
        preset.modules.len(), preset.footswitches.len(), data.len()
    );
    crate::current_preset::store_current_preset_info(preset.clone());
    Some(preset)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The frames must stay byte-identical to the read path that works, apart
    /// from the sequence number. Anything else changing here is a change to a
    /// sequence that is only known to work as captured.
    #[test]
    fn the_frames_match_the_proven_read_sequence() {
        // {102:1000, 100:76, 101:{}} — open the preset resource.
        assert_eq!(&OPEN_RESOURCE[24..33],
                   &[0x83, 0x66, 0xCD, 0x03, 0xE8, 0x64, 0x4C, 0x65, 0x80]);
        // {102:1012, 100:22, 101:nil} — stream the current preset.
        assert_eq!(&READ_PRESET[24..33],
                   &[0x83, 0x66, 0xCD, 0x03, 0xF4, 0x64, 0x16, 0x65, 0xC0]);
        // All three address x80, outbound byte order.
        for f in [&OPEN_RESOURCE[..], &READ_PRESET[..], &PULL_PAGE[..]] {
            assert_eq!(&f[4..8], &[0x80, 0x10, 0xED, 0x03]);
        }
        // Only byte 9 is filled in per use, so it starts clear.
        assert_eq!((OPEN_RESOURCE[9], READ_PRESET[9], PULL_PAGE[9]), (0, 0, 0));
    }

    /// Sequence numbers must climb, and start where the handshake left off.
    /// Reusing one is not a subtle problem: the device discards the frame and
    /// says nothing, so the read hangs.
    #[test]
    fn sequence_numbers_climb_from_the_handshake() {
        let seq = AtomicU8::new(3);
        let next = || seq.fetch_add(1, Ordering::SeqCst);
        assert_eq!((next(), next(), next()), (3, 4, 5));

        // A whole read consumes several, and the next read must not restart.
        let after_one_read: Vec<u8> = (0..20).map(|_| next()).collect();
        assert!(after_one_read.windows(2).all(|w| w[1] == w[0].wrapping_add(1)));
        assert_ne!(next(), 3);
    }

    /// Replay a real patch load and assemble it exactly as `read_preset_raw`
    /// does: skip anything that isn't an x80 payload frame, collect the rest,
    /// stop at the first page shorter than a full one.
    ///
    /// This pins the two things that decide whether loading works:
    ///
    /// * the first frame after the request is a **16-byte `cmd=08` frame**,
    ///   not data — taking it as the reply loses the head of the stream;
    /// * the stream ends at a **short page**, and what follows is a different
    ///   command's reply, which must not be swept in.
    #[test]
    fn a_captured_patch_load_assembles_the_way_the_read_loop_does() {
        let path = format!(
            "{}/captures/06-load-patch-01B-(A30 Fawn Brt).txt",
            env!("CARGO_MANIFEST_DIR")
        );
        let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
        let frames: Vec<(bool, Vec<u8>)> = text
            .lines()
            .filter_map(|line| {
                let mut f = line.split('\t');
                let (_, ep, hex) = (f.next()?, f.next()?, f.next()?);
                let hex: String = hex.chars().filter(|c| c.is_ascii_hexdigit()).collect();
                let b: Vec<u8> = (0..hex.len() / 2)
                    .map(|i| u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).unwrap())
                    .collect();
                (!b.is_empty()).then_some((ep == "0x01", b))
            })
            .collect();

        // The request: an outbound x80 command whose body is op 22.
        let start = frames
            .iter()
            .position(|(out, b)| {
                // {102:<txn>, 100:22, 101:nil} — the txn varies per session,
                // so match the op, not the number the capture happened to use.
                *out && b.len() > 32
                    && b[4..8] == [0x80, 0x10, 0xED, 0x03]
                    && b[24..27] == [0x83, 0x66, 0xCD]
                    && b[29] == 0x64 && b[30] == 0x16
            })
            .expect("the capture contains a preset-stream request");

        let mut data: Vec<u8> = vec![];
        let mut skipped_before_data = 0;
        let mut pages = 0;
        for (out, f) in &frames[start + 1..] {
            if *out || f.len() < 16 || f[4..8] != X80_IN {
                continue;
            }
            if f.len() == 16 {
                if data.is_empty() {
                    skipped_before_data += 1;
                }
                continue;
            }
            data.extend_from_slice(&f[16..]);
            pages += 1;
            if f.len() < PAGE {
                break;
            }
        }

        assert_eq!(skipped_before_data, 1, "a 16-byte frame precedes the stream");
        assert_eq!(pages, 16, "15 full pages and a short one");
        assert_eq!(data.len(), 15 * 256 + 100);
        // The 40-byte frame after the stream is another command's reply.
        assert!(data.len() % 4 == 0);
    }

    /// A frame is only ours if it is on x80 and carries a payload. The 16-byte
    /// heartbeat and other channels' traffic are neither an answer nor an
    /// end-of-stream.
    #[test]
    fn only_x80_payload_frames_are_answers() {
        let ours = |f: &[u8]| f.len() > 16 && f[4..8] == X80_IN;

        let mut page = vec![0u8; PAGE];
        page[4..8].copy_from_slice(&X80_IN);
        assert!(ours(&page));

        let mut heartbeat = vec![0u8; 16];
        heartbeat[4..8].copy_from_slice(&X80_IN);
        assert!(!ours(&heartbeat), "a heartbeat is not the end of a stream");

        let mut notification = vec![0u8; 52];
        notification[4..8].copy_from_slice(&[0xF0, 0x03, 0x02, 0x10]); // x2
        assert!(!ours(&notification), "another channel is not our reply");
    }
}
