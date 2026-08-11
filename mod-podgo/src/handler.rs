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

use crate::config::MAX_FX_PARAMS;

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

/// Map a device value onto the 0..=127 controller range its widget uses.
///
/// The device sends each param in **DSP units**, which differ per param — a
/// percent arrives as 0..1, a frequency as 20..20000 Hz, a level as -60..6 dB,
/// a note division as an integer index. `ParamDef::dsp_min`/`dsp_max` bound
/// that range (from Line 6's model data), so normalising against them handles
/// every kind uniformly instead of only the params that happen to be 0..1.
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
            ParamValue::Bool(b) => if *b { 127 } else { 0 },
            other => if as_f(other)? >= 0.5 { 127 } else { 0 },
        }),
        // A discrete param's wire value is its option index offset by dsp_min;
        // the combo box is driven by the raw index, not a 0..127 scale.
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
            Some((norm * 127.0).round() as u16)
        }
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

/// Apply something the device did to the UI.
///
/// Runs on the connection's reader thread. The value is stored with
/// **`Origin::MIDI`**, which is this codebase's marker for "the hardware told
/// us" — `generic::midi_cc_in_handler` does the same for every MIDI device,
/// and it is what stops the value being sent straight back once writing
/// exists.
fn apply_device_event(controller: &Arc<Mutex<Controller>>, event: crate::device::Event) {
    let crate::device::Event::Param { slot, index, value } = event else {
        if let crate::device::Event::Other { op } = event {
            debug!("Pod Go: undecoded device event, op {op}");
        }
        return;
    };

    let Some(ui) = ui_slot(slot) else {
        debug!("Pod Go: a change arrived for block {slot}, which is not in the chain");
        return;
    };
    if index as usize >= MAX_FX_PARAMS {
        return;
    }
    let prefix = crate::config::slot_prefix(ui);

    let mapped = {
        let ctrl = controller.lock().unwrap();
        let model = ctrl.get(&format!("{prefix}_select")).unwrap_or(0) as usize;
        crate::config::ALL_MODELS
            .get(model)
            .and_then(|m| m.params.param(index as usize))
            .and_then(|def| control_value(def, &value))
    };
    let Some(mapped) = mapped else {
        debug!("Pod Go: block {slot} param {index} has no matching control");
        return;
    };
    let name = format!("{prefix}_param{}", index + 1);
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
    const ATTEMPTS: u32 = 4;

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
