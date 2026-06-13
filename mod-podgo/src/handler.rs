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

use crate::config::{AMP_MODELS, CAB_MODELS, WAH_MODELS, EQ_MODELS, FX_MODELS, MAX_FX_PARAMS};

pub struct PodGoHandler;

impl Handler for PodGoHandler {
    fn new_device_handler(&self, ctx: &Ctx) {
        info!("Pod Go: initialised");

        let dump = ctx.dump.clone();
        tokio::spawn(async move {
            tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
            pod_usb::podgo_preset_names_init();
            if let Some(names) = pod_usb::podgo_preset_names() {
                let mut dump = dump.lock().unwrap();
                for (idx, name) in names {
                    dump.set_name(*idx as usize, name.clone(), Origin::MIDI);
                }
                info!("Loaded {} preset names from device", names.len());
            } else {
                warn!("No cached preset names available");
            }
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

                let controller = ctx.controller.clone();
                let program_num = program;
                tokio::spawn(async move {
                    tokio::time::sleep(Duration::from_millis(500)).await;

                    let preset = tokio::task::spawn_blocking(move || {
                        pod_usb::podgo_read_current_preset()
                    }).await.ok().flatten();

                    if let Some(ref preset) = preset {
                        sync_controller_from_preset(&controller, preset);
                        let names: Vec<String> = preset.modules.iter().map(|m| m.name.clone()).collect();
                        info!("Preset {} modules: {:?}", program_num, names);
                    }
                });
            }
        }
    }

    fn pc_handler(&self, ctx: &Ctx, event: &ProgramChangeEvent) {
        if event.origin == MIDI {
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

fn num_program(p: &Program) -> Option<usize> {
    match p {
        Program::ManualMode | Program::Tuner => None,
        Program::Program(v) => Some(*v as usize),
    }
}

fn sync_controller_from_preset(
    controller: &Arc<Mutex<Controller>>,
    preset: &pod_usb::PresetData,
) {
    for m in &preset.modules {
        info!("sync: module '{}' category '{}' slot {} bypassed={} params={:?}",
              m.name, m.category, m.slot, m.bypassed, m.parameters);
    }

    // Clear managed blocks first so anything absent from this preset doesn't
    // keep showing stale values from a previously-loaded preset.
    reset_managed_blocks(controller);

    // Walk the chain in slot order: fixed blocks go to their dedicated
    // controls; everything else is an assignable effect that fills the next
    // free FX slot (POD Go has four).
    let mut modules: Vec<&pod_usb::ModuleInfo> = preset.modules.iter().collect();
    modules.sort_by_key(|m| m.slot);

    let mut next_fx_slot = 0usize; // 0-based; slots 0..=3 map to fx1..fx4
    for m in &modules {
        let enable = if m.bypassed { 0u16 } else { 1u16 };

        if pod_usb::is_fixed_block_category(&m.category, &m.name) {
            match m.category.as_str() {
                "Amp" => {
                    set_select(controller, "amp_select",
                               AMP_MODELS.iter().position(|a| a.name == m.name), &m.name);
                    store_set(controller, "amp_enable", enable);
                }
                "Cab" => {
                    set_select(controller, "cab_select",
                               CAB_MODELS.iter().position(|n| *n == m.name), &m.name);
                }
                "Wah" => {
                    set_select(controller, "wah_select",
                               WAH_MODELS.iter().position(|n| *n == m.name), &m.name);
                    store_set(controller, "wah_enable", enable);
                }
                "Vol/Pan" => {
                    // Fixed Volume pedal block. No model selector; its level is
                    // a parameter. Just reflect the on/off state for now.
                    store_set(controller, "volume_enable", enable);
                }
                "EQ" => {
                    // Dedicated Preset EQ block (model selector for the EQ type).
                    set_select(controller, "preset_eq_select",
                               EQ_MODELS.iter().position(|n| *n == m.name), &m.name);
                    store_set(controller, "preset_eq_enable", enable);
                }
                _ => {
                    // FX Loop / Send-Return.
                    store_set(controller, "fx_loop_enable", enable);
                }
            }
            continue;
        }

        // Assignable effect block -> next free FX slot.
        if next_fx_slot >= 4 {
            warn!("sync: more than 4 FX blocks; '{}' (slot {}) not shown", m.name, m.slot);
            continue;
        }
        let n = next_fx_slot + 1;
        next_fx_slot += 1;
        set_select(controller, &format!("fx{}_select", n),
                   FX_MODELS.iter().position(|fm| fm.name == m.name), &m.name);
        store_set(controller, &format!("fx{}_enable", n), enable);
        sync_fx_params(controller, n, &m.name, &m.parameters);
    }
}

/// Reset all handler-managed block controls to a neutral state. Called before
/// syncing a preset so blocks not present in it don't display leftover values
/// from a previously-loaded preset.
fn reset_managed_blocks(controller: &Arc<Mutex<Controller>>) {
    // Fixed-block enables + the wah selector.
    const FIXED: &[&str] = &[
        "wah_select", "wah_enable",
        "volume_enable", "fx_loop_enable",
        "preset_eq_select", "preset_eq_enable",
    ];
    for c in FIXED {
        store_set(controller, c, 0);
    }
    // Every FX slot's selector, enable and all param sliders.
    for n in 1..=4 {
        store_set(controller, &format!("fx{}_select", n), 0);
        store_set(controller, &format!("fx{}_enable", n), 0);
        for p in 1..=MAX_FX_PARAMS {
            store_set(controller, &format!("fx{}_param{}", n, p), 0);
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
/// names (and that fit a 0..=127 percent control) are shown — unknown models
/// have an empty spec and therefore show no params yet.
fn sync_fx_params(
    controller: &Arc<Mutex<Controller>>,
    slot: usize,
    model_name: &str,
    params: &[pod_usb::ParamValue],
) {
    let Some(model) = FX_MODELS.iter().find(|m| m.name == model_name) else {
        return;
    };
    for (i, pv) in params.iter().enumerate() {
        if i >= MAX_FX_PARAMS || model.params.label(i).is_none() {
            continue;
        }
        let key = format!("fx{}_param{}", slot, i + 1);
        // The preset stores most params normalized to 0.0..=1.0, which map
        // directly onto the 0..=127 percent controls. Native-unit params
        // (Hz/dB/counts) don't fit those controls yet, so skip them for now.
        let value: u16 = match pv {
            pod_usb::ParamValue::Float(f) if (0.0..=1.0).contains(f) => {
                (*f * 127.0).round() as u16
            }
            pod_usb::ParamValue::Bool(b) => if *b { 127 } else { 0 },
            other => {
                info!("sync: {} = {:?} not a 0..1 value; needs a dedicated control, skipped", key, other);
                continue;
            }
        };
        store_set(controller, &key, value);
    }
}

fn store_set(controller: &Arc<Mutex<Controller>>, name: &str, value: u16) {
    let mut ctrl = controller.lock().unwrap();
    ctrl.set_full(name, value, pod_core::store::Origin::NONE, pod_core::store::Signal::Force);
}
