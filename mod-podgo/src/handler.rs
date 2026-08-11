use std::sync::{Arc, Mutex};
use std::time::Duration;
use log::*;
use pod_core::context::Ctx;
use pod_core::controller::*;
use pod_core::event::*;
use pod_core::handler::Handler;
use pod_core::midi::MidiMessage;
use pod_core::model::AbstractControl;
use pod_core::store::Store;
use Origin::{MIDI, UI};

use crate::config::{MAX_FX_PARAMS, PARAM_CARRIER};

pub struct PodGoHandler;

impl Handler for PodGoHandler {
    fn new_device_handler(&self, ctx: &Ctx) {
        info!("Pod Go: initialised");

        let dump = ctx.dump.clone();
        let controller = ctx.controller.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(2)).await;
            crate::podgo::fetch_and_cache();
            if let Some(names) = crate::podgo::preset_names() {
                let mut dump = dump.lock().unwrap();
                for (idx, name) in names {
                    dump.set_name(*idx as usize, name.clone(), Origin::MIDI);
                }
                info!("Loaded {} preset names from device", names.len());
            } else {
                warn!("No cached preset names available");
            }
            // The sink runs on the connection's reader thread, which is a plain
            // OS thread with no runtime of its own. Anything it wants to do
            // asynchronously — the resync a model change needs — has to be
            // handed back here.
            *RUNTIME.lock().unwrap_or_else(|e| e.into_inner()) =
                Some(tokio::runtime::Handle::current());

            // Open the connection that stays open, *after* the name fetch:
            // both claim USB interface 0, and from here on this owns it.
            // Changes the device makes on its own arrive through the sink.
            let c = controller.clone();
            let sink: Arc<dyn Fn(crate::device::Event) + Send + Sync> =
                Arc::new(move |event| apply_device_event(&c, event));
            let _ = tokio::task::spawn_blocking(move || crate::device::connect(sink)).await;

            // Show whatever the device already has loaded, rather than leaving
            // the panel blank until the user presses Load.
            refresh_from_device(controller, Duration::ZERO, "connect").await;
        });
    }

    fn load_handler(&self, ctx: &Ctx, event: &BufferLoadEvent) {
        if event.origin == UI {
            let program = match event.buffer {
                Buffer::Current => num_program(&ctx.program()),
                Buffer::Program(v) => Some(v),
                _ => None,
            };
            if let Some(program) = program {
                let msg = MidiMessage::ProgramChange {
                    channel: ctx.midi_channel(),
                    program: program as u8,
                };
                ctx.app_event_tx.send_or_warn(AppEvent::MidiMsgOut(msg));

                // Give the device a moment to act on the program change
                // before reading back what it loaded.
                tokio::spawn(refresh_from_device(
                    ctx.controller.clone(),
                    Duration::from_millis(500),
                    "preset load",
                ));
            }
        }
    }

    fn pc_handler(&self, ctx: &Ctx, event: &ProgramChangeEvent) {
        if event.origin == MIDI {
            // The preset was changed on the device itself. Nothing to send
            // back, but the edit buffer is now a different patch, so pull it in
            // — otherwise the UI keeps showing the previous one.
            let controller = ctx.controller.clone();
            tokio::spawn(refresh_from_device(
                controller, Duration::from_millis(500), "device program change",
            ));
            return;
        }

        let program = match event.program {
            Program::ManualMode => ctx.config.pc_manual_mode,
            Program::Tuner => ctx.config.pc_tuner,
            Program::Program(1000) => None,
            Program::Program(v) => {
                let offset = ctx.config.pc_offset.unwrap_or_default();
                Some(v as usize + offset)
            }
        };
        if let Some(program) = program {
            let msg = MidiMessage::ProgramChange {
                channel: ctx.midi_channel(),
                program: program as u8,
            };
            ctx.app_event_tx.send_or_warn(AppEvent::MidiMsgOut(msg));

            // Selecting a preset in the UI arrives here, *not* in
            // `load_handler` — that only runs for an explicit Load. Sending the
            // program change switches the device but tells us nothing about
            // what it now holds, so read the new edit buffer back.
            tokio::spawn(refresh_from_device(
                ctx.controller.clone(),
                Duration::from_millis(500),
                "program change",
            ));
        }
    }

    fn control_value_from_buffer(&self, controller: &mut Controller, name: &str, buffer: &[u8]) {
        let Some(control) = controller.get_config(name) else {
            return;
        };
        let Some((addr, len)) = control.get_addr() else {
            return;
        };
        let addr = addr as usize;
        let value = match len {
            1 => buffer[addr] as u32,
            2 => {
                let a = buffer[addr] as u32;
                let b = buffer[addr + 1] as u32;
                (a << 8) | b
            }
            4 => {
                let a = buffer[addr] as u32;
                let b = buffer[addr + 1] as u32;
                let c = buffer[addr + 2] as u32;
                let d = buffer[addr + 3] as u32;
                (a << 24) | (b << 16) | (c << 8) | d
            }
            n => {
                error!("Control width {} not supported!", n);
                0u32
            }
        };
        let value = control.value_from_buffer(value);
        controller.set(&name, value, StoreOrigin::NONE);
    }

    fn control_value_to_buffer(&self, controller: &Controller, name: &str, buffer: &mut [u8]) {
        let Some(control) = controller.get_config(name) else {
            return;
        };
        let Some((addr, len)) = control.get_addr() else {
            return;
        };
        let value = controller.get(name).unwrap();
        let value = control.value_to_buffer(value);
        let addr = addr as usize;
        match len {
            1 => {
                buffer[addr] = value as u8;
            }
            2 => {
                buffer[addr] = ((value >> 8) & 0xff) as u8;
                buffer[addr + 1] = (value & 0xff) as u8;
            }
            4 => {
                buffer[addr] = ((value >> 24) & 0xff) as u8;
                buffer[addr + 1] = ((value >> 16) & 0xff) as u8;
                buffer[addr + 2] = ((value >> 8) & 0xff) as u8;
                buffer[addr + 3] = (value & 0xff) as u8;
            }
            n => {
                error!("Control width {} not supported!", n);
            }
        }
    }
}




fn set_select(controller: &Arc<Mutex<Controller>>, control: &str, idx: Option<usize>, name: &str) {
    match idx {
        Some(idx) => {
            info!("sync: {} -> {} idx {}", name, control, idx);
            store_set(controller, control, idx as u16);
        }
        None => warn!("sync: '{}' not found for control '{}'", name, control),
    }
}

/// Map a parsed FX block's positional params onto its slot's `fxN_paramK`
/// controls, using the model's `ParamSpec` to decide which positions are
/// surfaced. Param values are already in device order; only those the spec
/// names are shown; an unknown model has an empty spec and shows no params.
fn sync_block_params(
    controller: &Arc<Mutex<Controller>>,
    prefix: &str,
    model_index: Option<usize>,
    params: &[crate::preset_parser::ParamValue],
) {
    let model = model_index.and_then(|i| crate::config::ALL_MODELS.get(i));
    // Clear every position first: a shorter model must not leave the previous
    // one's values behind.
    for k in 1..=MAX_FX_PARAMS {
        store_set(controller, &format!("{prefix}_param{k}"), 0);
    }
    let Some(model) = model else { return };
    for (i, pv) in params.iter().enumerate() {
        if i >= MAX_FX_PARAMS || model.params.label(i).is_none() {
            continue;
        }
        let Some(def) = model.params.param(i) else { continue };
        let key = format!("{}_param{}", prefix, i + 1);
        let Some(value) = control_value(def, pv) else {
            info!("sync: {} = {:?} has no mapping for kind {:?}, skipped", key, pv, def.kind);
            continue;
        };
        store_set(controller, &key, value);
    }
}

/// Map a device value onto the controller range its widget uses.
///
/// The device sends each param in **DSP units**, which differ per param — a
/// percent arrives as 0..1, a frequency as 20..20000 Hz, a level as -60..6 dB,
/// a note division as an integer index. `ParamDef::dsp_min`/`dsp_max` bound
/// that range (from Line 6's model data), so normalising against them handles
/// every kind uniformly instead of only the params that happen to be 0..1.
///
/// [`wire_value`] is the inverse and the two are tested as a round trip over
/// every parameter of every model — change one and change the other.
pub(crate) fn control_value(def: &crate::model::ParamDef, pv: &crate::preset_parser::ParamValue) -> Option<u16> {
    use crate::model::ParamKind;
    use crate::preset_parser::ParamValue;

    let as_f = |v: &ParamValue| match v {
        ParamValue::Float(f) => Some(*f as f64),
        ParamValue::Int(i) => Some(*i as f64),
        ParamValue::Bool(b) => Some(if *b { 1.0 } else { 0.0 }),
        ParamValue::Raw(_) => None,
    };

    match def.kind {
        ParamKind::Bool => Some(match pv {
            ParamValue::Bool(b) => if *b { PARAM_CARRIER } else { 0 },
            other => if as_f(other)? >= 0.5 { PARAM_CARRIER } else { 0 },
        }),
        // A discrete param's wire value is its option index offset by dsp_min;
        // the combo box is driven by the raw index, not by the carrier scale.
        ParamKind::Enum => {
            let idx = (as_f(pv)? - def.dsp_min).round();
            (idx >= 0.0).then(|| idx.min(def.options.len().saturating_sub(1) as f64) as u16)
        }
        ParamKind::Numeric => {
            let span = def.dsp_max - def.dsp_min;
            let norm = if span.abs() < f64::EPSILON {
                0.0
            } else {
                ((as_f(pv)? - def.dsp_min) / span).clamp(0.0, 1.0)
            };
            Some((norm * PARAM_CARRIER as f64).round() as u16)
        }
    }
}

/// Turn a controller value into the MessagePack scalar the device expects for
/// key 119. The inverse of [`control_value`].
///
/// Two things decide the result and they are independent:
///
/// * **`kind`** picks the arithmetic — an enum's carrier value already *is* its
///   option index and must never be scaled (the classic failure is sending a
///   note division of `6` as 600 %), while a numeric one is a position in
///   `dsp_min..dsp_max`.
/// * **`wire`** picks the MessagePack type. `7` and `7.0` are different values
///   on the wire and nothing coerces between them, so a semitone interval must
///   go out as an integer even though it shares a slider with percents.
pub(crate) fn wire_value(def: &crate::model::ParamDef, value: u16) -> crate::device::WireValue {
    use crate::device::WireValue;
    use crate::model::{ParamKind, WireType};

    let dsp = match def.kind {
        ParamKind::Bool => return WireValue::Bool(value > PARAM_CARRIER / 2),
        // Straight back to the device's numbering: label `options[v - dsp_min]`,
        // so the value for option `v` is `v + dsp_min`.
        ParamKind::Enum => value as f64 + def.dsp_min,
        ParamKind::Numeric => {
            let norm = (value as f64 / PARAM_CARRIER as f64).clamp(0.0, 1.0);
            def.dsp_min + norm * (def.dsp_max - def.dsp_min)
        }
    };

    match def.wire {
        WireType::Bool => WireValue::Bool(dsp >= 0.5),
        WireType::Int => WireValue::Int(dsp.round() as i64),
        // f32, not f64: every captured value is a 4-byte float, and an f64
        // would be a different MessagePack encoding of the same number.
        WireType::Float => WireValue::Float(dsp as f32),
    }
}

/// UI position (1-based) -> the device's number for that block.
static SLOT_MAP: Mutex<Vec<u8>> = Mutex::new(Vec::new());

fn set_slot_map(positions: &[usize]) {
    let mut m = SLOT_MAP.lock().unwrap_or_else(|e| e.into_inner());
    *m = positions.iter().map(|p| *p as u8).collect();
}

/// The 1-based UI position for a device block number.
fn ui_slot(device: u8) -> Option<usize> {
    let m = SLOT_MAP.lock().unwrap_or_else(|e| e.into_inner());
    m.iter().position(|s| *s == device).map(|i| i + 1)
}

/// The device's block number for a 1-based UI position — [`ui_slot`] reversed.
///
/// Commands address a block by its index in the preset's chain array, which is
/// what the UI's row was built from. The two have coincided in every preset
/// seen, but that is a property of those presets, so the mapping is read rather
/// than assumed. A position the current preset does not fill has no number, and
/// an edit there has nowhere to go.
pub(crate) fn device_slot(ui: usize) -> Option<u8> {
    let m = SLOT_MAP.lock().unwrap_or_else(|e| e.into_inner());
    m.get(ui.checked_sub(1)?).copied()
}

/// Everything the UI holds about one chain position: its model, its enable, and
/// its parameters. The order is arbitrary but fixed — only
/// [`read_position`]/[`write_position`] ever see it.
const POSITION_CONTROLS: usize = MAX_FX_PARAMS + 2;

fn position_control(prefix: &str, i: usize) -> String {
    match i {
        0 => format!("{prefix}_select"),
        1 => format!("{prefix}_enable"),
        _ => format!("{prefix}_param{}", i - 1),
    }
}

fn read_position(ctrl: &Controller, pos: usize) -> Vec<u16> {
    let prefix = crate::config::slot_prefix(pos);
    (0..POSITION_CONTROLS)
        .map(|i| ctrl.get(&position_control(&prefix, i)).unwrap_or(0))
        .collect()
}

fn write_position(ctrl: &mut Controller, pos: usize, values: &[u16]) {
    let prefix = crate::config::slot_prefix(pos);
    for (i, v) in values.iter().enumerate() {
        // `Origin::NONE` — the origin a preset load uses. It is what keeps a
        // reorder off the wire: `live::wire_writes` fires only on `Origin::UI`,
        // so re-pointing a span of positions does not send a parameter edit per
        // control. `live::write_model` leans on the same thing.
        ctrl.set(&position_control(&prefix, i), *v, pod_core::store::Origin::NONE);
    }
}

/// The highest position that holds a model, or 0 when the chain is empty.
///
/// Read from the controller rather than from [`SLOT_MAP`] so that reordering
/// works with no device connected. Index 0 of `ALL_MODELS` is the `(empty)`
/// entry, so "holds a model" is exactly "select is not 0".
pub(crate) fn last_occupied_position(ctrl: &Controller) -> usize {
    (1..=crate::config::CHAIN_SLOTS)
        .rev()
        .find(|p| {
            ctrl.get(&format!("{}_select", crate::config::slot_prefix(*p))).unwrap_or(0) != 0
        })
        .unwrap_or(0)
}

/// Move the block at position `from` to position `to`, shifting the positions
/// in between — the gesture POD Go Edit's drag performs.
///
/// Returns the position the block ended up at, or `None` if nothing moved.
///
/// # Why `SLOT_MAP` is not permuted alongside the values
///
/// [`device_slot`] maps a UI position to the block's index in the preset's chain
/// array, and that array is *positional* — its index is the running order. So a
/// move on the device renumbers the array, and UI position *i* still names chain
/// index *i* afterwards. Permuting the map here as well would apply the move
/// twice and send every later parameter edit to a neighbouring block, with
/// nothing to show for it in the UI. The re-read that follows a confirmed move
/// rebuilds the map from the preset in any case.
pub(crate) fn move_position(ctrl: &mut Controller, from: usize, to: usize) -> Option<usize> {
    let last = last_occupied_position(ctrl);
    if from == 0 || from > last {
        debug!("Pod Go: position {from} holds no block, not moving it");
        return None;
    }
    // Clamping rather than refusing: a drop past the end of a short chain is a
    // clear enough intention ("put it last"), and letting it through as-is would
    // leave a gap in the middle of the chain — the failure shape this format
    // invites, see `usb/docs/podgo-preset-format.md`.
    let to = to.clamp(1, last);
    if from == to {
        return None;
    }

    // Only the span between the two ends can move; everything outside it keeps
    // both its position and its values.
    let (lo, hi) = (from.min(to), from.max(to));
    let mut span: Vec<Vec<u16>> = (lo..=hi).map(|p| read_position(ctrl, p)).collect();
    let block = span.remove(from - lo);
    span.insert(to - lo, block);
    for (i, values) in span.iter().enumerate() {
        write_position(ctrl, lo + i, values);
    }

    match (device_slot(from), device_slot(to)) {
        (Some(df), Some(dt)) => {
            info!("Pod Go: move position {from} -> {to} (block {df} -> {dt})");
            crate::device::move_block(df, dt);
        }
        _ => info!("Pod Go: move position {from} -> {to}, which is not in the current chain"),
    }
    Some(to)
}

/// Blocks whose own writes should not be echoed back into the UI.
///
/// A write we send comes back on x2 carrying no mark of its origin — it is
/// indistinguishable from someone turning the knob on the pedal. Storing it
/// with `Origin::MIDI` already stops it being sent out again, so there is no
/// loop; what it does do is arrive *late*. During a drag the device is still
/// reporting values from several steps ago, and applying those moves the
/// slider backwards under the user's finger.
///
/// So an edit stamps its target here, and a report for that target is ignored
/// until the stamp goes stale. Reports for anything else — the pedal's own
/// knobs, another block — are unaffected.
static RECENTLY_WRITTEN: Mutex<Vec<((u8, u8, bool), std::time::Instant)>> = Mutex::new(Vec::new());

/// How long our own echo is suppressed for. Long enough to cover a drag's
/// round trip, short enough that letting go of a control and turning the same
/// knob on the pedal still registers.
const ECHO_WINDOW: Duration = Duration::from_millis(250);

/// Record that we just wrote this parameter.
pub(crate) fn mark_written(slot: u8, index: u8, ordinary: bool) {
    let mut m = RECENTLY_WRITTEN.lock().unwrap_or_else(|e| e.into_inner());
    let now = std::time::Instant::now();
    m.retain(|(_, at)| now.duration_since(*at) < ECHO_WINDOW);
    let key = (slot, index, ordinary);
    match m.iter_mut().find(|(k, _)| *k == key) {
        Some((_, at)) => *at = now,
        None => m.push((key, now)),
    }
}

/// Whether a report for this parameter is our own write coming back.
fn is_our_echo(slot: u8, index: u8, ordinary: bool) -> bool {
    let m = RECENTLY_WRITTEN.lock().unwrap_or_else(|e| e.into_inner());
    let now = std::time::Instant::now();
    m.iter().any(|(k, at)| {
        *k == (slot, index, ordinary) && now.duration_since(*at) < ECHO_WINDOW
    })
}

/// The runtime the reader thread hands asynchronous work back to.
///
/// Set once, while `new_device_handler`'s task is running — so it is available
/// before the connection whose reader thread uses it is opened.
static RUNTIME: Mutex<Option<tokio::runtime::Handle>> = Mutex::new(None);

/// A pending re-read of the edit buffer, and whether anything is waiting to
/// perform it.
struct Resync {
    /// The earliest moment the read should happen. Pushed back by every further
    /// report, so a burst becomes one read.
    due: Option<std::time::Instant>,
    running: bool,
}

static RESYNC: Mutex<Resync> = Mutex::new(Resync { due: None, running: false });

/// How long to wait for a block change to settle before reading the preset.
///
/// Turning the model knob on the pedal walks through models one at a time, each
/// reported separately; a preset read is about 4 kB over the same channel the
/// reports arrive on. Waiting for quiet turns a spin through twenty models into
/// one read of the model it stopped at.
const RESYNC_QUIET: Duration = Duration::from_millis(250);

/// Re-read the edit buffer, once, after things go quiet.
///
/// A model change is the one edit that cannot be applied incrementally: every
/// parameter of the block is replaced by the new model's defaults, and neither
/// the x2 report nor anything else on the wire carries them. Both directions
/// end up here — the device reporting its own change, and the writer confirming
/// ours — and coalesce into a single read.
fn request_resync(controller: &Arc<Mutex<Controller>>, why: &'static str) {
    {
        let mut r = RESYNC.lock().unwrap_or_else(|e| e.into_inner());
        r.due = Some(std::time::Instant::now() + RESYNC_QUIET);
        if r.running {
            return;
        }
        r.running = true;
    }

    let handle = RUNTIME.lock().unwrap_or_else(|e| e.into_inner()).clone();
    let Some(handle) = handle else {
        // Only reachable if a report arrives before the device handler has
        // started, which nothing currently allows. Say so rather than leave the
        // panel quietly stale.
        RESYNC.lock().unwrap_or_else(|e| e.into_inner()).running = false;
        warn!("Pod Go: {why} needs a re-read, but there is no runtime to do it on");
        return;
    };

    let controller = controller.clone();
    handle.spawn(async move {
        loop {
            let due = RESYNC.lock().unwrap_or_else(|e| e.into_inner()).due;
            match due {
                Some(due) => {
                    // `saturating_duration_since`, not `due - now`: the deadline
                    // can pass between reading it and subtracting, and `Instant`
                    // subtraction panics rather than saturating.
                    let wait = due.saturating_duration_since(std::time::Instant::now());
                    if !wait.is_zero() {
                        // More reports are still arriving; let them settle.
                        tokio::time::sleep(wait).await;
                        continue;
                    }
                    RESYNC.lock().unwrap_or_else(|e| e.into_inner()).due = None;
                    refresh_from_device(controller.clone(), Duration::ZERO, why).await;
                }
                None => {
                    // Checked and cleared under one lock, so a report that
                    // arrives now either sees `running` still set (and this
                    // loop picks it up) or spawns its own waiter.
                    let mut r = RESYNC.lock().unwrap_or_else(|e| e.into_inner());
                    if r.due.is_some() {
                        continue;
                    }
                    r.running = false;
                    break;
                }
            }
        }
    });
}

/// Apply something the device did to the UI.
///
/// Runs on the connection's reader thread. The value is stored with
/// **`Origin::MIDI`**, which is this codebase's marker for "the hardware told
/// us" — `generic::midi_cc_in_handler` does the same for every MIDI device,
/// and it is what stops the value being sent straight back once writing
/// exists.
fn apply_device_event(controller: &Arc<Mutex<Controller>>, event: crate::device::Event) {
    let (slot, index, ordinary, value) = match event {
        crate::device::Event::Param { slot, index, ordinary, value } => {
            (slot, index, ordinary, value)
        }
        crate::device::Event::Bypass { slot, enabled } => {
            let Some(ui) = ui_slot(slot) else { return };
            // A bypass has no parameter index; `u8::MAX` reserves a key for it
            // that no real index can collide with.
            if is_our_echo(slot, u8::MAX, true) {
                return;
            }
            let name = format!("{}_enable", crate::config::slot_prefix(ui));
            // Polarity is taken to match the preset's own "enabled" flag; if a
            // block shows the opposite of the pedal, this is the line.
            let mut ctrl = controller.lock().unwrap();
            ctrl.set(&name, u16::from(enabled), MIDI.into());
            return;
        }
        crate::device::Event::BlockChanged { slot } => {
            // Which model it now holds is not in the report, so there is
            // nothing to apply — only something to go and find out.
            info!(
                "Pod Go: block {slot} (position {:?}) changed model; re-reading the patch",
                ui_slot(slot)
            );
            request_resync(controller, "a model change");
            return;
        }
        crate::device::Event::Other { op } => {
            debug!("Pod Go: undecoded device event, op {op}");
            return;
        }
    };

    let Some(ui) = ui_slot(slot) else {
        debug!("Pod Go: a change arrived for block {slot}, which is not in the chain");
        return;
    };
    if index as usize >= MAX_FX_PARAMS {
        return;
    }
    // Our own edit, on its way back. Applying it would fight the control the
    // user is still holding.
    if is_our_echo(slot, index, ordinary) {
        return;
    }
    let prefix = crate::config::slot_prefix(ui);

    let mapped = {
        let ctrl = controller.lock().unwrap();
        let model = ctrl.get(&format!("{prefix}_select")).unwrap_or(0) as usize;
        let model = crate::config::ALL_MODELS.get(model);
        // The device's index counts in one of two lists — see `live_index`.
        let at = model.and_then(|m| m.params.live_index(index as usize, ordinary));
        let def = at.and_then(|at| model.and_then(|m| m.params.param(at)));
        // Name what the index was taken to mean. A change landing on the wrong
        // control is a mapping question, and this is the line that answers it
        // without having to reason about parameter order from a file.
        debug!(
            "Pod Go: block {slot} {} param {index} = {value:?} -> {} / {} (position {ui})",
            if ordinary { "ordinary" } else { "@" },
            model.map(|m| m.name.as_str()).unwrap_or("?"),
            def.map(|d| d.name.as_str()).unwrap_or("?"),
        );
        def.and_then(|def| control_value(def, &value)).map(|v| (at, v))
    };
    let Some((Some(at), mapped)) = mapped.map(|(a, v)| (a, v)) else {
        debug!("Pod Go: block {slot} param {index} has no matching control");
        return;
    };
    let name = format!("{prefix}_param{}", at + 1);
    let mut ctrl = controller.lock().unwrap();
    ctrl.set(&name, mapped, MIDI.into());
}

fn store_set(controller: &Arc<Mutex<Controller>>, name: &str, value: u16) {
    let mut ctrl = controller.lock().unwrap();
    ctrl.set_full(name, value, pod_core::store::Origin::NONE, pod_core::store::Signal::Force);
}

fn num_program(p: &Program) -> Option<usize> {
    match p {
        Program::ManualMode | Program::Tuner => None,
        Program::Program(v) => Some(*v as usize),
    }
}

/// Read the device's current edit buffer and push it into the UI.
///
/// Every path that can change what the device has loaded goes through here:
/// connecting, selecting or loading a preset in the UI, and the user changing
/// preset on the pedal itself. `delay` gives the device time to finish
/// switching before we read.
///
/// Retried a few times: the read claims the USB interface exclusively, so right
/// after connect it can lose the race with the preset-name fetch that runs just
/// before it. A single failed attempt used to leave the panel blank with no
/// indication why.
async fn refresh_from_device(
    controller: Arc<Mutex<Controller>>, delay: Duration, why: &'static str,
) {
    const ATTEMPTS: u32 = 2;

    if !delay.is_zero() {
        tokio::time::sleep(delay).await;
    }
    for attempt in 1..=ATTEMPTS {
        let preset = tokio::task::spawn_blocking(crate::device::read_preset)
            .await
            .ok()
            .flatten();
        if let Some(preset) = preset {
            info!("Read edit buffer after {why} (attempt {attempt})");
            sync_controller_from_preset(&controller, &preset);
            return;
        }
        warn!("Reading the edit buffer after {why} failed (attempt {attempt}/{ATTEMPTS})");
        tokio::time::sleep(Duration::from_millis(400 * attempt as u64)).await;
    }
    warn!("Giving up reading the edit buffer after {why}; the panel will stay stale until Load");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::device::WireValue;
    use crate::model::{ParamDef, ParamKind, WireType};
    use crate::preset_parser::ParamValue;

    /// The DSP values worth checking for one parameter: both ends, and enough
    /// of the middle to catch a scale that is off rather than merely shifted.
    fn samples(def: &ParamDef) -> Vec<f64> {
        match def.kind {
            ParamKind::Bool => vec![0.0, 1.0],
            // Every option, since an index is meaningless if any of them
            // misses. `options[v - dsp_min]`, so option n is value n + dsp_min.
            ParamKind::Enum => (0..def.options.len()).map(|i| i as f64 + def.dsp_min).collect(),
            ParamKind::Numeric if def.wire == WireType::Int => {
                // Every value the parameter can actually take, capped so a
                // wide counter doesn't dominate the test's runtime.
                let (lo, hi) = (def.dsp_min.round() as i64, def.dsp_max.round() as i64);
                let step = ((hi - lo) / 64).max(1);
                (lo..=hi).step_by(step as usize).map(|v| v as f64).collect()
            }
            ParamKind::Numeric => [0.0, 0.1, 0.25, 0.5, 0.75, 0.9, 1.0]
                .iter()
                .map(|t| def.dsp_min + t * (def.dsp_max - def.dsp_min))
                .collect(),
        }
    }

    /// A value the device sent, shown in the UI and sent straight back, must be
    /// the value it sent.
    ///
    /// This is the property that matters and it is checked over **every
    /// parameter of every model** — 5723 of them — because the failures it
    /// catches are silent. A percent that round-trips and an enum that does not
    /// look identical in the UI; the difference only appears on the pedal, as a
    /// note division that jumps to a different value the moment anything else
    /// on that block is touched.
    ///
    /// It also pins [`control_value`] and [`wire_value`] to each other. They are
    /// two halves of one mapping and there is no way to change one correctly on
    /// its own.
    #[test]
    fn a_value_read_from_the_device_writes_back_unchanged() {
        let mut checked = 0usize;
        for model in crate::config::ALL_MODELS.iter() {
            for (i, def) in model.params.iter().enumerate() {
                if def.kind == ParamKind::Enum && def.options.is_empty() {
                    continue;
                }
                for dsp in samples(def) {
                    let read = match def.wire {
                        WireType::Bool => ParamValue::Bool(dsp >= 0.5),
                        // The preset parser coerces every stored integer to a
                        // float, so this is the shape an enum really arrives in
                        // — and the shape that used to get scaled as a percent.
                        _ => ParamValue::Float(dsp as f32),
                    };
                    let carrier = control_value(def, &read)
                        .unwrap_or_else(|| panic!("{}/{} has no carrier value for {dsp}",
                                                  model.name, def.name));
                    assert!(
                        carrier <= PARAM_CARRIER,
                        "{}/{}: {dsp} mapped to {carrier}, past the carrier",
                        model.name, def.name
                    );

                    let back = wire_value(def, carrier);
                    let ok = match back {
                        WireValue::Bool(b) => b == (dsp >= 0.5),
                        WireValue::Int(v) => v as f64 == dsp.round(),
                        // One carrier step of slack, which is all the round trip
                        // can lose: the value is quantised to a position and
                        // read back out of it.
                        WireValue::Float(v) => {
                            let step = (def.dsp_max - def.dsp_min).abs() / PARAM_CARRIER as f64;
                            (v as f64 - dsp).abs() <= step + 1e-4 * dsp.abs().max(1.0)
                        }
                    };
                    assert!(
                        ok,
                        "{}/{} (param {i}, {:?}/{:?}, dsp {}..{}): sent {dsp}, \
                         carried as {carrier}, came back {back:?}",
                        model.name, def.name, def.kind, def.wire, def.dsp_min, def.dsp_max
                    );
                    checked += 1;
                }
            }
        }
        assert!(checked > 20_000, "only {checked} values checked; the models did not load");
    }

    /// A discrete parameter must never leave as a float.
    ///
    /// `119` is polymorphic and the device does not coerce: a note division
    /// sent as `6.0` is not the `6` that means "1/4". The two ways to get this
    /// wrong are scaling the index like a percent, and sending the right number
    /// in the wrong MessagePack type — this rules out the second for every
    /// model at once.
    #[test]
    fn discrete_parameters_are_never_sent_as_floats() {
        for model in crate::config::ALL_MODELS.iter() {
            for def in model.params.iter() {
                match def.kind {
                    ParamKind::Enum => assert_eq!(
                        def.wire, WireType::Int,
                        "{}/{} is a dropdown but would be sent as {:?}",
                        model.name, def.name, def.wire
                    ),
                    ParamKind::Bool => assert_eq!(
                        def.wire, WireType::Bool,
                        "{}/{} is a checkbox but would be sent as {:?}",
                        model.name, def.name, def.wire
                    ),
                    ParamKind::Numeric => assert_ne!(
                        def.wire, WireType::Bool,
                        "{}/{} is a slider but would be sent as a bool",
                        model.name, def.name
                    ),
                }
                // And the value itself, at both ends of the range.
                if def.kind == ParamKind::Enum {
                    for carrier in [0u16, def.options.len().saturating_sub(1) as u16] {
                        match wire_value(def, carrier) {
                            WireValue::Int(v) => assert_eq!(
                                v as f64, carrier as f64 + def.dsp_min,
                                "{}/{}: option {carrier} must send as its own index",
                                model.name, def.name
                            ),
                            other => panic!("{}/{} sent {other:?}", model.name, def.name),
                        }
                    }
                }
            }
        }
    }

    /// The two directions of parameter addressing must be exact inverses.
    ///
    /// The read path takes `(index, key 29)` from the device and finds our
    /// parameter; the write path does the reverse. If they disagree, an edit
    /// silently lands on a different parameter of the same block — and a cab's
    /// mic type and its Distance, both "index 0" of different lists, are
    /// exactly the pair that would swap.
    #[test]
    fn wire_index_inverts_live_index() {
        let mut with_specials = 0;
        for model in crate::config::ALL_MODELS.iter() {
            let spec = &model.params;
            if spec.iter().any(|p| p.special) {
                with_specials += 1;
            }
            for i in 0..spec.len() {
                let (index, ordinary) = spec
                    .wire_index(i)
                    .unwrap_or_else(|| panic!("{} param {i} has no device index", model.name));
                assert_eq!(
                    spec.live_index(index as usize, ordinary), Some(i),
                    "{}: param {i} -> ({index}, ordinary={ordinary}) -> somewhere else",
                    model.name
                );
            }
        }
        assert!(with_specials > 0, "no model has @-parameters; the test proves nothing");
    }

    // --- reordering --------------------------------------------------------

    /// A controller holding the captured patch, and the ten positions' models.
    fn loaded() -> Arc<Mutex<Controller>> {
        let controller = Arc::new(Mutex::new(Controller::new(crate::config::CONFIG.controls.clone())));
        let data = include_bytes!("../tests/fixtures/a30-fawn-brt.preset.bin");
        let preset = crate::preset_parser::parse_preset_data(data);
        sync_controller_from_preset(&controller, &preset);
        controller
    }

    fn models(ctrl: &Controller) -> Vec<&'static str> {
        (1..=crate::config::CHAIN_SLOTS)
            .map(|p| {
                let i = ctrl.get(&format!("{}_select", crate::config::slot_prefix(p))).unwrap();
                crate::config::ALL_MODELS[i as usize].name.as_str()
            })
            .collect()
    }

    /// Every control of every position, named explicitly rather than through
    /// [`read_position`] — a test that reads the state through the same helper
    /// the code under test writes it with cannot see that helper miss a control.
    fn everything(ctrl: &Controller) -> Vec<(String, u16)> {
        let mut all = vec![];
        for p in 1..=crate::config::CHAIN_SLOTS {
            let prefix = crate::config::slot_prefix(p);
            let mut names = vec![format!("{prefix}_select"), format!("{prefix}_enable")];
            names.extend((1..=MAX_FX_PARAMS).map(|k| format!("{prefix}_param{k}")));
            for name in names {
                let v = ctrl.get(&name).unwrap_or_else(|| panic!("no control {name}"));
                all.push((name, v));
            }
        }
        all
    }

    /// One position's controls, by name, in the same independent way.
    fn position(ctrl: &Controller, p: usize) -> Vec<(String, u16)> {
        let prefix = crate::config::slot_prefix(p);
        everything(ctrl)
            .into_iter()
            .filter(|(name, _)| name.starts_with(&format!("{prefix}_")))
            .map(|(name, v)| (name[prefix.len()..].to_string(), v))
            .collect()
    }

    /// The captured patch fills all ten positions, so it exercises the whole row.
    const LOADED_ORDER: [&str; 10] = [
        "Fassel", "Volume", "FX Loop 1", "Top Secret OD", "A30 Fawn Brt",
        "2x12 Blue Bell", "LA Studio Comp", "Transistor Tape", "Room",
        "Parametric [STATIC]",
    ];

    /// Dropping 3 onto 7 puts the block at 7 and shifts 4..7 one place left.
    #[test]
    fn moving_a_block_later_shifts_the_span_left() {
        let controller = loaded();
        let mut ctrl = controller.lock().unwrap();
        assert_eq!(models(&ctrl), LOADED_ORDER, "fixture");

        assert_eq!(move_position(&mut ctrl, 3, 7), Some(7));
        assert_eq!(
            models(&ctrl),
            [
                "Fassel", "Volume", "Top Secret OD", "A30 Fawn Brt", "2x12 Blue Bell",
                "LA Studio Comp", "FX Loop 1", "Transistor Tape", "Room",
                "Parametric [STATIC]",
            ]
        );
    }

    /// And the other way, which shifts the span the other way — a separate case
    /// because `remove`/`insert` move the intervening entries in the opposite
    /// direction, and an off-by-one there is invisible in the forward test.
    #[test]
    fn moving_a_block_earlier_shifts_the_span_right() {
        let controller = loaded();
        let mut ctrl = controller.lock().unwrap();

        assert_eq!(move_position(&mut ctrl, 7, 3), Some(3));
        assert_eq!(
            models(&ctrl),
            [
                "Fassel", "Volume", "LA Studio Comp", "FX Loop 1", "Top Secret OD",
                "A30 Fawn Brt", "2x12 Blue Bell", "Transistor Tape", "Room",
                "Parametric [STATIC]",
            ]
        );
    }

    /// A block's values travel with it. Moving only the model selector would
    /// pass both tests above and leave every block wearing its neighbour's
    /// settings.
    #[test]
    fn a_moved_block_takes_its_values_with_it() {
        let controller = loaded();
        let mut ctrl = controller.lock().unwrap();
        let before = position(&ctrl, 8);
        // Transistor Tape has eleven values; a block whose parameters were all
        // zero would make this test vacuous.
        assert!(
            before.iter().filter(|(n, _)| n.starts_with("_param")).any(|(_, v)| *v != 0),
            "the block has no values to carry",
        );

        move_position(&mut ctrl, 8, 2);
        assert_eq!(position(&ctrl, 2), before);
    }

    /// There and back leaves the chain exactly as it was — every position, every
    /// control, not just the models.
    #[test]
    fn a_move_and_its_reverse_restore_the_chain() {
        let controller = loaded();
        let mut ctrl = controller.lock().unwrap();
        let before = everything(&ctrl);

        move_position(&mut ctrl, 3, 7);
        assert_ne!(everything(&ctrl), before, "the move did nothing");
        move_position(&mut ctrl, 7, 3);
        assert_eq!(everything(&ctrl), before);
    }

    /// The map from UI position to device block number must survive a move
    /// untouched: the chain array is positional, so the device renumbers it
    /// itself and position *i* still names block *i*. Permuting it here as well
    /// would double-apply the move and misaddress every later edit — silently,
    /// since the UI would look right.
    #[test]
    fn a_move_leaves_the_device_slot_map_alone() {
        let controller = loaded();
        let before: Vec<Option<u8>> =
            (1..=crate::config::CHAIN_SLOTS).map(device_slot).collect();
        assert!(before.iter().all(|s| s.is_some()), "the fixture fills every position");

        move_position(&mut controller.lock().unwrap(), 3, 7);

        let after: Vec<Option<u8>> = (1..=crate::config::CHAIN_SLOTS).map(device_slot).collect();
        assert_eq!(after, before);
    }

    /// Moves that cannot mean anything change nothing.
    #[test]
    fn degenerate_moves_are_refused() {
        let controller = loaded();
        let mut ctrl = controller.lock().unwrap();
        let before = everything(&ctrl);

        assert_eq!(move_position(&mut ctrl, 4, 4), None, "a block onto itself");
        assert_eq!(move_position(&mut ctrl, 0, 4), None, "no position 0");
        assert_eq!(
            move_position(&mut ctrl, crate::config::CHAIN_SLOTS + 1, 4), None,
            "past the end of the row",
        );
        assert_eq!(everything(&ctrl), before);
    }

    /// With a short chain, a drop past the last block lands on the last block
    /// rather than leaving a hole in the middle — the failure shape this format
    /// invites (`usb/docs/podgo-preset-format.md`).
    #[test]
    fn a_drop_past_the_end_of_a_short_chain_clamps() {
        let controller = loaded();
        let mut ctrl = controller.lock().unwrap();
        // Empty the last three positions, leaving a seven-block chain.
        for p in 8..=crate::config::CHAIN_SLOTS {
            write_position(&mut ctrl, p, &vec![0; POSITION_CONTROLS]);
        }
        assert_eq!(last_occupied_position(&ctrl), 7);

        assert_eq!(move_position(&mut ctrl, 2, 10), Some(7), "clamped to the last block");
        assert_eq!(
            models(&ctrl)[..8],
            [
                "Fassel", "FX Loop 1", "Top Secret OD", "A30 Fawn Brt", "2x12 Blue Bell",
                "LA Studio Comp", "Volume", "(empty)",
            ],
            "no gap opens between the blocks",
        );
        // And dragging one of the empty positions moves nothing.
        assert_eq!(move_position(&mut ctrl, 9, 3), None);
    }
}

/// Push a preset into the controller, position by position.
///
/// The chain is the unit of work: each position states which model it holds and
/// what its values are, which is everything the UI needs. Nothing here consults
/// a block's *category*, so no block can fail to be placed and no position can
/// be claimed by something else — the failure that made unrecognised blocks
/// (Loopers, Send/Return variants) shift every later block one place left.
///
/// Category is a property of the model, not of the position; it matters only
/// when changing what a position holds.
pub(crate) fn sync_controller_from_preset(
    controller: &Arc<Mutex<Controller>>,
    preset: &crate::preset_parser::PresetData,
) {
    // Positions come from the preset's own chain, not from a fixed index range.
    let positions = preset.block_positions();
    // The device names blocks by their index in this chain; the UI by position
    // in its row. Publish the correspondence rather than assume the two agree.
    set_slot_map(&positions);
    for slot in 1..=crate::config::CHAIN_SLOTS {
        let prefix = crate::config::slot_prefix(slot);
        let block = positions.get(slot - 1).and_then(|i| preset.chain.get(*i));
        let index = block
            .and_then(|b| b.model_id)
            .and_then(crate::config::model_index_for_id);

        // One line per position, always: the id the chain reported, the name
        // the preset carried (if any), and what that resolved to. Anything
        // showing as empty is then attributable to a specific step rather than
        // guessed at.
        match (block.and_then(|b| b.model_id), index) {
            (Some(id), Some(i)) => info!(
                "sync: position {slot} id={id} name={:?} -> {:?}",
                block.and_then(|b| b.name.as_deref()),
                crate::config::ALL_MODELS[i].name
            ),
            (Some(id), None) => warn!(
                "sync: position {slot} id={id} name={:?} -> NOT IN MODEL DATABASE",
                block.and_then(|b| b.name.as_deref())
            ),
            (None, _) if block.map(|b| b.name.is_some()).unwrap_or(false) => warn!(
                "sync: position {slot} name={:?} has NO MODEL ID in its chain meta \
                 (block[20][24][25]); shown as empty",
                block.and_then(|b| b.name.as_deref())
            ),
            (None, _) => info!("sync: position {slot} empty"),
        }

        // Index 0 is the explicit "(empty)" entry, so an unfilled or unresolved
        // position clears rather than showing the previous preset's model.
        store_set(controller, &format!("{prefix}_select"), index.unwrap_or(0) as u16);
        store_set(
            controller,
            &format!("{prefix}_enable"),
            u16::from(matches!(block, Some(b) if b.model_id.is_some() && !b.bypassed)),
        );

        let params = block.map(|b| b.parameters.as_slice()).unwrap_or(&[]);
        sync_block_params(controller, &prefix, index, params);

    }
}
