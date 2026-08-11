//! The connection to the device: opened once, kept open.
//!
//! USB interface 0 has exactly one owner, so everything that talks to the POD
//! Go has to go through one place. This is that place. It is opened when the
//! device is detected and held for as long as the program runs — reading a
//! patch no longer costs a connect, a handshake and a disconnect.
//!
//! **Reading only so far.** A reader thread takes every frame the device sends,
//! so a knob turned on the pedal arrives the instant it happens — nothing here
//! polls on a timer. Writing edits comes later; see
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
//! * **The flow-control count must be maintained, on both channels.** Bytes
//!   12..16 of every outbound frame restate how much of that channel's output
//!   has been taken, counting from [`podgo_session::CREDIT_BASE`]. Let it go
//!   stale and the device stops answering: on x2 it stops reporting after
//!   about twenty events, and on x80 it stops serving patches — which reads as
//!   a patch that will not load rather than as a protocol fault.
//!
//! # Falling back
//!
//! Loading is what already worked, and it is never allowed to depend on this.
//! If a read over the open connection fails twice, the connection is
//! **released first** and the read retried on the original one-shot path.
//! Releasing first is not optional: that path opens its own connection, and it
//! cannot while this one is held — its handshake would just time out.

use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU8, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;

use log::*;

use crate::podgo_session;
use crate::preset_parser::{self, ParamValue, PresetData};

const EP_OUT: u8 = 0x01;
const EP_IN: u8 = 0x81;

/// A full stream page. A shorter one is the last.
const PAGE: usize = 272;

/// The channels, as the four bytes at 4..8 of an inbound frame. Outbound
/// frames carry the same four reversed, which is easy to get wrong.
const X80_IN: [u8; 4] = [0xED, 0x03, 0x80, 0x10];
const X2_IN: [u8; 4] = [0xF0, 0x03, 0x02, 0x10];

/// Ask the device for anything it has to report on x2.
///
/// **The device answers requests; it does not push.** Capture 09 shows the
/// host sending this before every report it receives, and a connection that
/// never sends one gets nothing — the channel stays alive, answers nothing,
/// and looks broken.
///
/// This is not a refresh timer. Once reports start, each acknowledgement
/// solicits the next, so a burst arrives with no delay; the interval only
/// decides how long after going idle a request is outstanding again.
const POLL_X2: [u8; 16] = [
    0x08,0,0,0x18, 0x02,0x10,0xF0,0x03, 0,0, 0,0x10, 0,0,0,0,
];

/// The same request on the edit-buffer channel.
///
/// Capture 09 shows the editor polling **all three** channels while idle, not
/// just the notification one. We only ever spoke on x80 during a read, so
/// after a patch reload — when the device has things to say on it — nothing
/// was outstanding for it to answer, and the next read went unanswered while
/// x2 carried on reporting normally.
const POLL_X80: [u8; 16] = [
    0x08,0,0,0x18, 0x80,0x10,0xED,0x03, 0,0, 0,0x10, 0,0,0,0,
];

/// How often to ask while idle. POD Go Edit uses roughly this.
const POLL_INTERVAL: Duration = Duration::from_millis(100);

/// Acknowledge an x2 notification. Byte 9 is the host's **own** sequence
/// number — the two sides count independently, and this does not echo the
/// device's — and bytes 12..16 the running count of x2 payload taken, which is
/// what keeps the device sending past the first twenty or so events.
const ACK_X2: [u8; 16] = [
    0x08,0,0,0x18, 0x02,0x10,0xF0,0x03, 0,0, 0,0x08, 0,0,0,0,
];

/// Open the preset resource.
///
/// The bytes are the read path's, with two fields filled in per use: byte 9,
/// the sequence number, and bytes 12..16, the count of what we have taken from
/// this channel. The `0x1009` sitting in the template is what that count
/// happens to be for the first read of a fresh connection — which is why a
/// constant worked for years of one-shot reads and stops working the moment a
/// connection is reused.
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

/// The rest of POD Go Edit's connect, replayed.
///
/// Capture 01 is a *complete* connect, and in that same session the device
/// reports knob turns (captures 09–11) — so the editor needs nothing beyond
/// this, and no "notify mode" open appears anywhere in it.
///
/// **Replaying it is not sufficient, though.** Tested on hardware: the device
/// accepts every one of these (status 0) and still reports nothing. So the
/// editor relies on something here that we are reproducing incorrectly, or on
/// something outside this script entirely. Kept because it is what the editor
/// does and it is harmless; [`SUBSCRIBE`] is what actually starts reporting.
///
/// `(channel, cmd, op, payload)`, in the order the editor sends them. Their
/// replies are property reads — `{63:false}`, `{118:14, 119:1}` — so nothing
/// here changes the patch.
fn post_connect_script() -> Vec<(Chan, u8, u64, rmpv::Value)> {
    use rmpv::Value;
    let map = |pairs: Vec<(u64, Value)>| {
        Value::Map(pairs.into_iter().map(|(k, v)| (Value::from(k), v)).collect())
    };
    vec![
        (Chan::X80, 0x0C, 23,  Value::Nil),
        (Chan::X1,  0x04, 254, map(vec![])),
        (Chan::X80, 0x04, 99,  map(vec![])),
        (Chan::X1,  0x04, 1,   map(vec![(107, Value::from(0)), (101, Value::from(2))])),
        (Chan::X80, 0x0C, 24,  map(vec![(118, Value::from(14))])),
        (Chan::X80, 0x0C, 24,  map(vec![(118, Value::from(73))])),
        (Chan::X1,  0x04, 13,  map(vec![(101, Value::from(2))])),
    ]
}

#[derive(Clone, Copy, PartialEq)]
enum Chan {
    X1,
    X80,
}

impl Chan {
    /// Outbound header bytes, and the channel id in the `01 00 <id> 00` tuple.
    fn header(&self) -> ([u8; 4], u8) {
        match self {
            Chan::X1 => ([0x01, 0x10, 0xEF, 0x03], 5),
            Chan::X80 => ([0x80, 0x10, 0xED, 0x03], 6),
        }
    }
}

/// Build a command frame. Bytes 12..16 carry the value the working read path
/// uses for that channel; the device accepts a stale one on command frames.
fn command_frame(
    chan: Chan, seq: u8, cmd: u8, txn: u32, op: u64, payload: rmpv::Value, credit: Option<u32>,
) -> Vec<u8> {
    use rmpv::Value;
    let body = Value::Map(vec![
        (Value::from(102u64), Value::from(txn)),
        (Value::from(100u64), Value::from(op)),
        (Value::from(101u64), payload),
    ]);
    let mut encoded = vec![];
    rmpv::encode::write_value(&mut encoded, &body).expect("in-memory encode");

    let (ch, id) = chan.header();
    let field: u32 = credit.unwrap_or(if chan == Chan::X1 { 0x0000_1009 } else { 0x0000_100F });
    let dlen = encoded.len() as u32;
    let f = field.to_le_bytes();
    let mut p = vec![
        (dlen + 16) as u8, 0, 0, 0x18,
        ch[0], ch[1], ch[2], ch[3],
        0, seq, 0, cmd,
        f[0], f[1], f[2], f[3],
        1, 0, id, 0,
    ];
    p.extend_from_slice(&dlen.to_le_bytes());
    p.extend_from_slice(&encoded);
    while p.len() % 4 != 0 {
        p.push(0);
    }
    p
}

/// Ask the device to report changes: the same open as `READ_PRESET` with a
/// payload of `0` instead of nil, selecting notify mode.
///
/// POD Go Edit does not send this — its connect script (replayed above) is
/// accepted by the device but does not start reporting, so whatever the editor
/// relies on is still undecoded. This command *is* confirmed on hardware, by
/// the `podgo_listen_probe` session: after it, turning a knob pushes.
///
/// Sent after a read, never before. It opens the same resource the read
/// streams from, and an earlier attempt that sent it first had every read come
/// back empty — so if patches stop loading, this is why.
const SUBSCRIBE: [u8; 36] = [
    0x19,0,0,0x18, 0x80,0x10,0xED,3, 0,0, 0,0x0C,
    0x0F,0x10,0x00,0, 1,0,6,0, 9,0,0,0,
    0x83,0x66,0xCD,3,0xF4,0x64,0x16,0x65,0x00,0,0,0,
];

/// Ask for the next page.
const PULL_PAGE: [u8; 16] = [
    0x08,0,0,0x18, 0x80,0x10,0xED,3, 0,0, 0,8, 0x0F,0x10,0x00,0,
];

/// Something the device reported of its own accord.
#[derive(Clone, Debug)]
pub enum Event {
    /// A parameter moved on the device itself.
    ///
    /// `ordinary` is key 29: whether the index counts in the block's ordinary
    /// parameters or in its `@`-prefixed ones. Both lists start at zero, so
    /// without it a cab's Distance and its mic type are indistinguishable.
    Param { slot: u8, index: u8, ordinary: bool, value: ParamValue },
    /// A block was switched on or off.
    ///
    /// Reported under key **59**, not 119 — capture 02 shows
    /// `{105:49, 106:{…, 106:{98:3, 59:true}}}`. Looking only for 119 made
    /// every one of these fall through as undecodable.
    Bypass { slot: u8, enabled: bool },
    /// Something else changed. We cannot decode it, and guessing is worse than
    /// re-reading the patch.
    Other { op: i64 },
}

/// Where events go. Set once, when the connection opens.
static SINK: Mutex<Option<Arc<dyn Fn(Event) + Send + Sync>>> = Mutex::new(None);

fn emit(event: Event) {
    let sink = SINK.lock().unwrap_or_else(|e| e.into_inner()).clone();
    if let Some(sink) = sink {
        sink(event);
    }
}

struct Inner {
    handle: rusb::DeviceHandle<rusb::Context>,
    /// Monotonic for the life of the connection. The handshake uses 0 and 2 on
    /// x80, so the first frame after it is 3.
    seq_x80: AtomicU8,
    /// The host's own x2 counter, independent of the device's.
    seq_x2: AtomicU8,
    /// The setlist channel's counter. The handshake uses 0, 2, 3 and 4 on x1.
    seq_x1: AtomicU8,
    /// Transaction ids for commands, echoed by the device in its replies.
    txn: AtomicU32,
    /// Payload bytes taken from x2, restated in every acknowledgement.
    credit_x2: AtomicU32,
    /// The same for x80.
    ///
    /// This was left as a constant for a long time because the read frames
    /// carried one and reads worked. They worked because the constant *was*
    /// the right count for the first read of a fresh connection. Hold the
    /// connection open, read a patch or two, and the device has sent thousands
    /// of bytes we never account for — the window closes and it stops
    /// answering, which looks like a patch that suddenly will not load.
    credit_x80: AtomicU32,
    alive: AtomicBool,
    /// Where x80 payload frames go while a preset is being read.
    reading: Mutex<Option<mpsc::Sender<Vec<u8>>>>,
    /// Held while writing, so frames from different threads cannot interleave.
    writing: Mutex<()>,
}

impl Drop for Inner {
    fn drop(&mut self) {
        let _ = self.handle.release_interface(0);
        debug!("Pod Go: connection closed");
    }
}

pub struct Device {
    inner: Arc<Inner>,
    /// Both threads are joined on the way out. The poller holds an `Arc` too,
    /// so leaving it running keeps the connection — and the interface — alive
    /// past the point where anything else may claim it.
    threads: Vec<std::thread::JoinHandle<()>>,
}

impl Drop for Device {
    fn drop(&mut self) {
        // Stop the reader and wait for it, so the interface is genuinely free
        // by the time this returns — anything that opens its own connection
        // (the one-shot fallback) cannot start until it is.
        self.inner.alive.store(false, Ordering::SeqCst);
        for h in self.threads.drain(..) {
            let _ = h.join();
        }
    }
}

impl Device {
    /// Claim the interface, run the channel handshake, start reading.
    pub fn open() -> Option<Device> {
        let handle = match podgo_session::find_and_open_podgo() {
            Ok(h) => h,
            Err(e) => {
                warn!("Pod Go: cannot open the device: {e}");
                return None;
            }
        };
        let credits = match podgo_session::session_init(&handle) {
            Ok(c) => c,
            Err(e) => {
                warn!("Pod Go: handshake failed: {e}");
                let _ = handle.release_interface(0);
                return None;
            }
        };

        let inner = Arc::new(Inner {
            handle,
            seq_x80: AtomicU8::new(3),
            seq_x2: AtomicU8::new(3),
            seq_x1: AtomicU8::new(5),
            txn: AtomicU32::new(2000),
            // Seeded from the handshake, and from the right base — see
            // `podgo_session::CREDIT_BASE`.
            credit_x2: AtomicU32::new(credits.x2),
            credit_x80: AtomicU32::new(credits.x80),
            alive: AtomicBool::new(true),
            reading: Mutex::new(None),
            writing: Mutex::new(()),
        });

        let reader = {
            let inner = inner.clone();
            std::thread::spawn(move || read_loop(inner))
        };
        // Keep a request outstanding on the notification channel, so the
        // device has something to answer the moment a knob moves.
        let poller = {
            let inner = inner.clone();
            std::thread::spawn(move || {
                while inner.alive.load(Ordering::SeqCst) {
                    inner.poll_x2();
                    inner.poll_x80();
                    std::thread::sleep(POLL_INTERVAL);
                }
            })
        };
        let device = Device { inner, threads: vec![reader, poller] };

        // The probe that confirmed reporting did this immediately, before any
        // streaming read. Doing it later has been tried and does not report.
        if subscribe_when() == "early" {
            device.subscribe();
        }
        Some(device)
    }

    fn send(&self, frame: &[u8]) -> bool {
        self.inner.send(frame)
    }

    /// Read the current preset, returning the raw assembled bytes.
    pub fn read_preset_raw(&self) -> Option<Vec<u8>> {
        // Take delivery of x80 payload frames for the duration.
        let (tx, rx) = mpsc::channel::<Vec<u8>>();
        *self.inner.reading.lock().unwrap_or_else(|e| e.into_inner()) = Some(tx);
        let data = self.stream_preset(&rx);
        *self.inner.reading.lock().unwrap_or_else(|e| e.into_inner()) = None;

        data
    }

    /// Ask the device to report changes, mirroring the sequence the probe
    /// confirmed on hardware: open the resource, then open it again in notify
    /// mode, back to back with nothing in between.
    ///
    /// `PODGO_SUBSCRIBE` selects when: `early` (the default) does it straight
    /// after the handshake, as the probe did; `late` waits until a patch has
    /// been read; `off` skips it. This is switchable because the two known
    /// facts conflict — notify mode is what makes the device report, and it
    /// also stops the resource streaming — and one run of each settles which
    /// order, if any, gives both.
    pub fn subscribe(&self) {
        let mut open = OPEN_RESOURCE;
        open[9] = self.inner.next_seq_x80();
        open[12..16].copy_from_slice(&self.inner.credit_x80.load(Ordering::SeqCst).to_le_bytes());
        if !self.send(&open) {
            return;
        }
        let mut sub = SUBSCRIBE;
        sub[9] = self.inner.next_seq_x80();
        sub[12..16].copy_from_slice(&self.inner.credit_x80.load(Ordering::SeqCst).to_le_bytes());
        if self.send(&sub) {
            debug!("Pod Go: asked the device to report changes");
        }
    }

    /// Finish connecting the way POD Go Edit does, once the first patch has
    /// been read. Sent after, not before, so a problem here can never stop the
    /// patch that is already loading.
    pub fn finish_connect(&self) {
        for (chan, cmd, op, payload) in post_connect_script() {
            let seq = match chan {
                Chan::X1 => self.inner.next_seq_x1(),
                Chan::X80 => self.inner.next_seq_x80(),
            };
            let txn = self.inner.next_txn();
            let credit = match chan {
                Chan::X1 => None, // x1 is only used here; leave its constant
                Chan::X80 => Some(self.inner.credit_x80.load(Ordering::SeqCst)),
            };
            let frame = command_frame(chan, seq, cmd, txn, op, payload, credit);
            if !self.send(&frame) {
                warn!("Pod Go: could not send connect op {op}");
                return;
            }
            // The replies come back through the reader; nothing waits on them.
            std::thread::sleep(Duration::from_millis(20));
        }
        if subscribe_when() == "late" {
            self.subscribe();
        }
    }

    fn stream_preset(&self, rx: &mpsc::Receiver<Vec<u8>>) -> Option<Vec<u8>> {
        let mut open = OPEN_RESOURCE;
        open[9] = self.inner.next_seq_x80();
        open[12..16].copy_from_slice(&self.inner.credit_x80.load(Ordering::SeqCst).to_le_bytes());
        if !self.send(&open) {
            return None;
        }
        rx.recv_timeout(Duration::from_millis(1500)).ok()?;

        let mut request = READ_PRESET;
        request[9] = self.inner.next_seq_x80();
        request[12..16]
            .copy_from_slice(&self.inner.credit_x80.load(Ordering::SeqCst).to_le_bytes());
        if !self.send(&request) {
            return None;
        }

        // The reply to the request is the head of the stream; every page after
        // it is asked for. A page shorter than a full one is the last.
        let mut data: Vec<u8> = vec![];
        let mut page = rx.recv_timeout(Duration::from_millis(1500)).ok()?;
        loop {
            data.extend_from_slice(&page[16..]);
            if page.len() < PAGE {
                break;
            }
            let mut pull = PULL_PAGE;
            pull[9] = self.inner.next_seq_x80();
            pull[12..16]
                .copy_from_slice(&self.inner.credit_x80.load(Ordering::SeqCst).to_le_bytes());
            if !self.send(&pull) {
                break;
            }
            match rx.recv_timeout(Duration::from_millis(700)) {
                Ok(next) => page = next,
                Err(_) => break,
            }
        }

        (!data.is_empty()).then_some(data)
    }

    /// Whether the reader is still running.
    pub fn alive(&self) -> bool {
        self.inner.alive.load(Ordering::SeqCst)
    }
}

impl Inner {
    fn next_seq_x80(&self) -> u8 {
        self.seq_x80.fetch_add(1, Ordering::SeqCst)
    }

    fn next_seq_x1(&self) -> u8 {
        self.seq_x1.fetch_add(1, Ordering::SeqCst)
    }

    fn next_txn(&self) -> u32 {
        self.txn.fetch_add(1, Ordering::SeqCst)
    }

    /// Send a frame. Serialised: the reader acknowledges notifications while
    /// the poller asks for more and a read pulls pages, so three threads reach
    /// this endpoint.
    fn send(&self, frame: &[u8]) -> bool {
        let _writing = self.writing.lock().unwrap_or_else(|e| e.into_inner());
        self.handle.write_bulk(EP_OUT, frame, Duration::from_millis(500)).is_ok()
    }

    /// Ask for whatever the device has to report.
    fn poll_x2(&self) {
        let mut poll = POLL_X2;
        poll[9] = self.seq_x2.fetch_add(1, Ordering::SeqCst);
        poll[12..16].copy_from_slice(&self.credit_x2.load(Ordering::SeqCst).to_le_bytes());
        self.send(&poll);
    }

    /// The same for the edit-buffer channel, but never while a read is in
    /// flight — that conversation has its own requests, and a second one
    /// interleaved would be answered out of turn.
    fn poll_x80(&self) {
        if self.reading.lock().unwrap_or_else(|e| e.into_inner()).is_some() {
            return;
        }
        let mut poll = POLL_X80;
        poll[9] = self.next_seq_x80();
        poll[12..16].copy_from_slice(&self.credit_x80.load(Ordering::SeqCst).to_le_bytes());
        self.send(&poll);
    }

    /// Account for x80 payload the device has sent us.
    ///
    /// The window is [`podgo_session::CREDIT_BASE`] — 4096 — bytes wide, and a
    /// single preset is about 4000 of them. Seeding this at the handshake and
    /// never adding to it therefore buys exactly one read: the second goes over
    /// the window and the device stops serving x80 altogether. It does not
    /// refuse, it just says nothing, which reads as a patch that will not load.
    fn took_x80(&self, frame: &[u8]) {
        self.credit_x80.fetch_add(podgo_session::credit_of(frame), Ordering::SeqCst);
    }

    /// Acknowledge one x2 notification.
    fn ack_x2(&self, frame: &[u8]) {
        let credit = self.credit_x2.fetch_add(podgo_session::credit_of(frame), Ordering::SeqCst)
            + podgo_session::credit_of(frame);
        let mut ack = ACK_X2;
        ack[9] = self.seq_x2.fetch_add(1, Ordering::SeqCst);
        ack[12..16].copy_from_slice(&credit.to_le_bytes());
        self.send(&ack);
    }
}

/// Take everything the device sends, for as long as the connection lives.
///
/// This is why notifications are immediate: the thread is parked in the USB
/// read, so a knob turn arrives the moment the device sends it. Nothing here
/// polls on a timer.
fn read_loop(inner: Arc<Inner>) {
    let mut buf = [0u8; 8192];
    let mut errors = 0u32;
    // Enough to tell "the device says nothing" from "it speaks and we ignore
    // it" without a running commentary. PODGO_TRACE_FRAMES=1 logs every frame.
    let trace = std::env::var("PODGO_TRACE_FRAMES").is_ok_and(|v| v != "0");
    let (mut x2_seen, mut x80_seen, mut idle_seen, mut other_seen) = (0u64, 0u64, 0u64, 0u64);
    let mut reported = std::time::Instant::now();
    debug!("Pod Go: reader started");

    while inner.alive.load(Ordering::SeqCst) {
        let n = match inner.handle.read_bulk(EP_IN, &mut buf, Duration::from_millis(200)) {
            Ok(n) => n,
            Err(rusb::Error::Timeout) => continue,
            Err(rusb::Error::NoDevice) => {
                warn!("Pod Go: the device went away");
                inner.alive.store(false, Ordering::SeqCst);
                break;
            }
            Err(e) => {
                errors += 1;
                // Bursts arrive milliseconds apart and throw the occasional
                // Pipe/Overflow; clearing and carrying on is the difference
                // between a reader that survives and one that stops.
                if matches!(e, rusb::Error::Pipe | rusb::Error::Overflow) {
                    let _ = inner.handle.clear_halt(EP_IN);
                }
                if errors == 1 || errors % 200 == 0 {
                    warn!("Pod Go: read error #{errors}: {e:?} (recovering)");
                }
                if errors > 1000 {
                    error!("Pod Go: giving up reading after {errors} errors");
                    inner.alive.store(false, Ordering::SeqCst);
                    break;
                }
                std::thread::sleep(Duration::from_millis(2));
                continue;
            }
        };
        errors = 0;
        let frame = &buf[..n];

        // Tally what arrives, so silence and noise are distinguishable.
        if n >= 8 {
            match (&frame[4..8], n) {
                (_, 0..=16) => idle_seen += 1,
                (c, _) if c == X2_IN => x2_seen += 1,
                (c, _) if c == X80_IN => x80_seen += 1,
                _ => other_seen += 1,
            }
        }
        if trace && n >= 16 {
            debug!(
                "Pod Go: <- {} len={n} seq={:02x} cmd={:02x}",
                match &frame[4..8] {
                    c if c == X2_IN => "x2 ",
                    c if c == X80_IN => "x80",
                    [0xEF, 0x03, 0x01, 0x10] => "x1 ",
                    _ => "???",
                },
                frame[9], frame[11]
            );
        }
        if reported.elapsed() > Duration::from_secs(20) {
            debug!(
                "Pod Go: heard from the device — {x2_seen} change reports, \
                 {x80_seen} replies, {idle_seen} idle, {other_seen} other"
            );
            reported = std::time::Instant::now();
        }

        if n <= 16 {
            continue; // heartbeat and flow-control frames carry nothing
        }

        // Acknowledgements arrive on whichever channel the command went out
        // on. Logging only x80 left the x1 commands unaccounted for — absence
        // of a log line is not absence of a reply.
        if frame[4..8] != X2_IN && frame[4..8] != X80_IN {
            if let Some((txn, status)) = ack_of(frame) {
                debug!("Pod Go: reply to command {txn}, status {status} (x1)");
            }
            continue;
        }

        if frame[4..8] == X2_IN {
            inner.ack_x2(frame);
            if x2_seen == 1 {
                info!("Pod Go: the device is reporting its own changes");
            }
            match decode_event(frame) {
                Some(e) => {
                    debug!("Pod Go: device change {e:?}");
                    emit(e);
                }
                None => debug!("Pod Go: an x2 frame did not decode: {}", hex(frame)),
            }
        } else if frame[4..8] == X80_IN {
            // Before anything else, and before the next page is asked for: the
            // pull frame restates this count, and a stale one closes the window.
            inner.took_x80(frame);
            let reading = inner.reading.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(tx) = reading.as_ref() {
                let _ = tx.send(frame.to_vec());
            }
            // Outside a read this is a command acknowledgement. The connect
            // script's replies land here, and whether the device accepted
            // those is exactly what we want to know.
            drop(reading);
            if let Some((txn, status)) = ack_of(frame) {
                debug!("Pod Go: reply to command {txn}, status {status}");
            }
        }
    }
    debug!("Pod Go: reader stopped");
}

/// Decode a notification.
///
/// Shape: `{105:op, 106:{82:_, 68:_, 121:_, 106:{98:slot, 28:index, 119:value}}}`.
/// The inner map is the same one a set-parameter command carries, so it is
/// found by looking for key 119 rather than by walking a fixed path.
fn decode_event(frame: &[u8]) -> Option<Event> {
    let mut cur = body_of(frame)?;
    let value = rmpv::decode::read_value(&mut cur).ok()?;
    let op = value.as_map()
        .and_then(|m| m.iter().find(|(k, _)| k.as_u64() == Some(105)))
        .and_then(|(_, v)| v.as_i64())
        .unwrap_or(-1);

    let Some(inner) = find_param_map(&value) else {
        return Some(Event::Other { op });
    };
    let get = |k: u64| inner.iter().find(|(key, _)| key.as_u64() == Some(k)).map(|(_, v)| v);
    let slot = get(98).and_then(|v| v.as_u64());

    // A block switched on or off carries key 59 and nothing else.
    if let (Some(slot), Some(enabled)) = (slot, get(59).and_then(|v| v.as_bool())) {
        return Some(Event::Bypass { slot: slot as u8, enabled });
    }

    let index = get(28).and_then(|v| v.as_u64());
    // Coerced exactly as the preset parser coerces stored values, so a value
    // that arrives this way and one that is read back mean the same thing.
    let param = get(119).and_then(|v| match v {
        rmpv::Value::Boolean(b) => Some(ParamValue::Bool(*b)),
        rmpv::Value::F32(f) => Some(ParamValue::Float(*f)),
        rmpv::Value::F64(f) => Some(ParamValue::Float(*f as f32)),
        rmpv::Value::Integer(i) => i.as_f64().map(|n| ParamValue::Float(n as f32)),
        _ => None,
    });

    // Key 29 chooses the list. Captures 09 and 11 carry true for ordinary
    // parameters; capture 10, a mic-type change, carries false.
    let ordinary = get(29).and_then(|v| v.as_bool()).unwrap_or(true);

    match (slot, index, param) {
        (Some(slot), Some(index), Some(value)) => Some(Event::Param {
            slot: slot as u8,
            index: index as u8,
            ordinary,
            value,
        }),
        _ => Some(Event::Other { op }),
    }
}

/// A frame's MessagePack body, if it has one.
///
/// The prologue is 24 bytes, and frames shorter than that carry none — a reply
/// of `nil` comes back as 20 bytes. Slicing without checking panics, and a
/// panic here kills the reader thread, after which the device seems to go
/// silent for reasons that have nothing to do with the device.
fn body_of(frame: &[u8]) -> Option<&[u8]> {
    (frame.len() > 24).then(|| &frame[24..])
}

/// The `{102:txn, 103:status}` an acknowledgement carries.
fn ack_of(frame: &[u8]) -> Option<(u32, i64)> {
    let mut cur = body_of(frame)?;
    let v = rmpv::decode::read_value(&mut cur).ok()?;
    let m = v.as_map()?;
    let get = |k: u64| m.iter().find(|(key, _)| key.as_u64() == Some(k)).map(|(_, v)| v);
    Some((get(102)?.as_u64()? as u32, get(103)?.as_i64()?))
}

fn hex(b: &[u8]) -> String {
    b.iter().take(48).map(|x| format!("{x:02x}")).collect()
}

/// The innermost map describing what changed: the one naming a block (98) and
/// carrying either a parameter value (119) or a bypass flag (59).
fn find_param_map(v: &rmpv::Value) -> Option<&Vec<(rmpv::Value, rmpv::Value)>> {
    let m = v.as_map()?;
    let has = |k: u64| m.iter().any(|(key, _)| key.as_u64() == Some(k));
    if has(98) && (has(119) || has(59)) {
        return Some(m);
    }
    m.iter().find_map(|(_, val)| find_param_map(val))
}

/// The one connection, for as long as the program runs.
static DEVICE: Mutex<Option<Device>> = Mutex::new(None);

/// Whether the editor's full connect script has been replayed yet.
static CONNECTED_FULLY: AtomicBool = AtomicBool::new(false);

/// `early` (default), `late`, or `off` — see [`Device::subscribe`].
fn subscribe_when() -> String {
    if SUBSCRIBE_GIVEN_UP.load(Ordering::SeqCst) {
        return "off".into();
    }
    // Tested both ways: it does not make the device report, and it does stop
    // the resource streaming, so a patch read fails afterwards. Off unless
    // someone asks for it.
    std::env::var("PODGO_SUBSCRIBE").unwrap_or_else(|_| "off".into())
}

/// Set once asking for change reports has cost us two patch reads.
///
/// Notify mode is what makes the device report, and it also stops the resource
/// streaming, so the two can be in direct conflict. Loading matters more:
/// after two failures the request is abandoned for the rest of the run, and
/// the connection settles into something that reads reliably.
static SUBSCRIBE_GIVEN_UP: AtomicBool = AtomicBool::new(false);

/// Consecutive read failures over the open connection.
static FAILED_READS: AtomicU32 = AtomicU32::new(0);

fn held() -> std::sync::MutexGuard<'static, Option<Device>> {
    // A poisoned lock must not disable the device for the rest of the run.
    DEVICE.lock().unwrap_or_else(|e| e.into_inner())
}

/// Open the connection. Safe to call again; does nothing if one is already up.
///
/// Retried, for the same reason reading retries: claiming interface 0 straight
/// after the preset-name fetch has released it loses a race often enough to
/// matter.
pub fn connect(sink: Arc<dyn Fn(Event) + Send + Sync>) -> bool {
    if held().is_some() {
        return true;
    }
    *SINK.lock().unwrap_or_else(|e| e.into_inner()) = Some(sink);
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

/// Reopen without the noise of a first connect.
fn connect_quietly() -> bool {
    if held().is_some() {
        return true;
    }
    match Device::open() {
        Some(dev) => {
            *held() = Some(dev);
            true
        }
        None => false,
    }
}

pub fn is_connected() -> bool {
    held().is_some()
}

/// Close the connection and release the interface.
pub fn disconnect() {
    CONNECTED_FULLY.store(false, Ordering::SeqCst);
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
                // One retry, not a staircase of them. A read that works answers
                // in tens of milliseconds; a read that fails twice in a row is
                // not going to be fixed by waiting longer, and the caller
                // retries too — the two loops multiplied out to twelve reads
                // and the better part of a minute before anything reached the
                // UI. If this ever needs patience again, the cause is a closed
                // flow-control window, not a slow device.
                dev.read_preset_raw().or_else(|| {
                    debug!("Pod Go: the preset did not read, retrying once");
                    std::thread::sleep(Duration::from_millis(150));
                    dev.read_preset_raw()
                })
            }
            None => None,
        }
    };

    if let Some(data) = over_connection {
        FAILED_READS.store(0, Ordering::SeqCst);
        let preset = parse_and_store(&data);
        // Once, after the first patch: the rest of the editor's connect, which
        // is what makes the device report knob turns.
        if preset.is_some() && !CONNECTED_FULLY.swap(true, Ordering::SeqCst) {
            if let Some(dev) = held().as_ref() {
                dev.finish_connect();
            }
        }
        return preset;
    }

    let was_connected = is_connected();
    if was_connected {
        let failures = FAILED_READS.fetch_add(1, Ordering::SeqCst) + 1;
        warn!("Pod Go: reading over the open connection failed ({failures}), falling back");
        if failures >= 2 && !SUBSCRIBE_GIVEN_UP.swap(true, Ordering::SeqCst) {
            warn!(
                "Pod Go: giving up on change reports — asking for them is what \
                 stops the device streaming a patch, and loading matters more"
            );
        }
        // Release before anything else claims the interface, and let it settle:
        // the fallback opens its own connection and re-runs the handshake, and
        // doing that the instant the previous one closed can time out.
        disconnect();
        std::thread::sleep(Duration::from_millis(250));
    }

    let preset = crate::current_preset::read_current_preset_inprocess();

    // Come back. Without this one failure leaves the program with no
    // connection and no reader for the rest of the run, so nothing can be
    // reported even once the cause has passed.
    if was_connected {
        std::thread::sleep(Duration::from_millis(150));
        if connect_quietly() {
            debug!("Pod Go: connection re-established");
        }
    }
    preset
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

    /// One preset does not fit in the window, so the count **must** climb.
    ///
    /// This is the whole of the "first patch loads, every one after it hangs"
    /// bug. `credit_x80` was seeded from the handshake and never added to,
    /// which works for as long as the device's window lasts — and a single
    /// preset is about 4000 bytes against a 4096-byte window, so it lasts for
    /// exactly one read. The second goes over, the device stops serving x80
    /// without saying so, and the read times out.
    #[test]
    fn one_preset_read_all_but_exhausts_the_x80_window() {
        use crate::podgo_session::{credit_of, CREDIT_BASE};

        let path = format!(
            "{}/captures/06-load-patch-01B-(A30 Fawn Brt).txt",
            env!("CARGO_MANIFEST_DIR")
        );
        let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
        let taken: u32 = text
            .lines()
            .filter_map(|line| {
                let mut f = line.split('\t');
                let (_, ep, hex) = (f.next()?, f.next()?, f.next()?);
                if ep == "0x01" {
                    return None; // outbound
                }
                let hex: String = hex.chars().filter(|c| c.is_ascii_hexdigit()).collect();
                let b: Vec<u8> = (0..hex.len() / 2)
                    .map(|i| u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).unwrap())
                    .collect();
                (b.len() >= 16 && b[4..8] == X80_IN).then(|| credit_of(&b))
            })
            .sum();

        // Comfortably inside the window once, and nowhere near it twice.
        assert!(taken < CREDIT_BASE, "one read took {taken}, window is {CREDIT_BASE}");
        assert!(
            taken * 2 > CREDIT_BASE,
            "two reads must overrun the window, or this bug could not happen"
        );

        // So a counter that never moves reports a full window on the second
        // read, and one that is maintained reports what was actually taken.
        let stale = AtomicU32::new(CREDIT_BASE);
        let maintained = AtomicU32::new(CREDIT_BASE);
        for _ in 0..2 {
            maintained.fetch_add(taken, Ordering::SeqCst);
        }
        assert_eq!(stale.load(Ordering::SeqCst), CREDIT_BASE);
        assert!(maintained.load(Ordering::SeqCst) > CREDIT_BASE + CREDIT_BASE);
    }

    /// Every x2 payload frame in a capture, decoded.
    fn events_in(name: &str) -> Vec<Event> {
        let path = format!("{}/captures/{name}", env!("CARGO_MANIFEST_DIR"));
        let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
        text.lines()
            .filter_map(|line| {
                let mut f = line.split('\t');
                let (_, ep, hex) = (f.next()?, f.next()?, f.next()?);
                if ep == "0x01" {
                    return None;
                }
                let hex: String = hex.chars().filter(|c| c.is_ascii_hexdigit()).collect();
                let b: Vec<u8> = (0..hex.len() / 2)
                    .map(|i| u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).unwrap())
                    .collect();
                (b.len() > 24 && b[4..8] == X2_IN).then(|| decode_event(&b))?
            })
            .collect()
    }

    /// Turning Chamber's Mix knob on the pedal, 30 % to 80 %. Nothing was sent
    /// to the device in this capture — every frame is the device reporting
    /// itself, which is exactly what Phase 2 has to understand.
    #[test]
    fn a_knob_turned_on_the_device_decodes() {
        let events = events_in("09-device-param-change(Chamber-Mix-30-to-80).txt");
        let params: Vec<(u8, u8, f32)> = events
            .iter()
            .filter_map(|e| match e {
                Event::Param { slot, index, value: ParamValue::Float(v), .. } => {
                    Some((*slot, *index, *v))
                }
                _ => None,
            })
            .collect();

        assert!(params.len() > 20, "got {} events", params.len());
        // Chamber sits at block 10; Mix is its parameter 4.
        assert!(params.iter().all(|(s, i, _)| *s == 10 && *i == 4), "{params:?}");
        let lo = params.iter().map(|(_, _, v)| *v).fold(f32::MAX, f32::min);
        let hi = params.iter().map(|(_, _, v)| *v).fold(f32::MIN, f32::max);
        assert!(lo <= 0.32 && hi >= 0.79, "swept {lo}..{hi}, expected about 0.30..0.80");
    }

    /// A discrete parameter changed on the device — the cab's mic type, three
    /// steps. These arrive as integers, and the preset parser coerces stored
    /// integers to floats, so both paths must agree on what a value means.
    #[test]
    fn a_discrete_change_on_the_device_decodes() {
        let events = events_in("10-device-enum-change(Cab-Mic-change-type-3-times-from-0-to-3).txt");
        let values: Vec<f32> = events
            .iter()
            .filter_map(|e| match e {
                Event::Param { value: ParamValue::Float(v), .. } => Some(*v),
                _ => None,
            })
            .collect();
        assert!(!values.is_empty(), "capture 10 reports mic-type changes");
        // Indices, so whole numbers — not a fraction of a range.
        assert!(values.iter().all(|v| (v - v.round()).abs() < 1e-6), "{values:?}");
    }

    /// Short frames must decode to nothing rather than panic.
    ///
    /// A reply of `nil` — which is what two of the connect ops return — comes
    /// back as 20 bytes: past the 16-byte heartbeat guard, but with no body at
    /// all. Slicing at 24 panicked, and because that happens on the reader
    /// thread it took the reader with it, after which the device appeared to
    /// go silent for reasons that had nothing to do with the device.
    #[test]
    fn frames_too_short_to_hold_a_body_decode_to_nothing() {
        for len in [16usize, 17, 20, 24] {
            let mut frame = vec![0u8; len];
            if len >= 8 {
                frame[4..8].copy_from_slice(&X80_IN);
            }
            assert_eq!(ack_of(&frame), None, "{len}-byte frame");
            assert!(decode_event(&frame).is_none(), "{len}-byte frame");
        }
        // 25 bytes is a body of one byte: decodable, if meaningless.
        let mut frame = vec![0u8; 25];
        frame[4..8].copy_from_slice(&X80_IN);
        let _ = ack_of(&frame);
    }

    /// The credit a channel starts at, and what it is worth getting wrong.
    ///
    /// Capture 01: the channel open carries `0x1000`, and the next frame
    /// `0x1009` — the base plus nine bytes. Reporting a bare `9` instead tells
    /// the device the buffer is nearly full, and it responds by splitting each
    /// notification into four-byte fragments that decode to nothing.
    #[test]
    fn the_credit_starts_at_a_base_not_at_zero() {
        use crate::podgo_session::{credit_of, CREDIT_BASE};
        assert_eq!(CREDIT_BASE, 0x1000);

        // The 28-byte reply the channel open is followed by is worth 9.
        let mut reply = vec![0u8; 28];
        reply[0] = 0x11;
        assert_eq!(credit_of(&reply), 9);
        assert_eq!(CREDIT_BASE + credit_of(&reply), 0x1009);

        // Which is exactly what the read frames have always hardcoded.
        assert_eq!(u32::from_le_bytes(OPEN_RESOURCE[12..16].try_into().unwrap()), 0x1009);
        assert_eq!(u32::from_le_bytes(READ_PRESET[12..16].try_into().unwrap()), 0x100F);
    }

    /// Switching a block off is reported under key 59, not 119. Decoding only
    /// 119 made every one of these look undecodable, which is why enabling and
    /// disabling a block did nothing in the panel.
    #[test]
    fn a_block_switched_off_decodes_as_bypass() {
        // The frame from capture 02: {105:49, 106:{82:0, 68:5, 121:17,
        //                             106:{98:3, 59:true}}}
        let body = [
            0x82u8, 0x69, 0x31, 0x6A, 0x84, 0x52, 0x00, 0x44, 0x05, 0x79, 0x11,
            0x6A, 0x82, 0x62, 0x03, 0x3B, 0xC3,
        ];
        let mut frame = vec![0u8; 24];
        frame[4..8].copy_from_slice(&X2_IN);
        frame.extend_from_slice(&body);

        match decode_event(&frame) {
            Some(Event::Bypass { slot, enabled }) => {
                assert_eq!(slot, 3);
                assert!(enabled);
            }
            other => panic!("expected a bypass event, got {other:?}"),
        }
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
