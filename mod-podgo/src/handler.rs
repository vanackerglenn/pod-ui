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

use crate::config::{STOMP_CONFIG, MOD_CONFIG, DELAY_CONFIG, REVERB_MODELS};
use crate::model::ConfigAccess;

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
        info!("sync: module '{}' category '{}' slot {} params={:?}",
              m.name, m.category, m.slot, m.parameters);
        match m.category.as_str() {
            "Distortion" | "Distortion (Legacy)" | "Dynamic" | "EQ" | "Filter"
            | "Wah" | "Pitch/Synth" | "Vol/Pan" => {
                if let Some(idx) = STOMP_CONFIG.iter().position(|c| *c.name() == m.name) {
                    info!("sync: {} -> stomp_select idx {}", m.name, idx);
                    store_set(controller, "stomp_select", idx as u16);
                } else {
                    warn!("sync: {} not found in STOMP_CONFIG", m.name);
                }
                if let Some(entry) = STOMP_CONFIG.iter().find(|c| *c.name() == m.name) {
                    sync_params(controller, entry.labels(), "stomp", &m.parameters);
                }
            }
            "Reverb" => {
                    if let Some(idx) = REVERB_MODELS.iter().position(|n| *n == m.name) {
                    info!("sync: {} -> reverb_select idx {}", m.name, idx);
                    store_set(controller, "reverb_select", idx as u16);
                } else {
                    warn!("sync: {} not found in REVERB_MODELS", m.name);
                }
            }
            "Modulation" => {
                if let Some(idx) = MOD_CONFIG.iter().position(|c| *c.name() == m.name) {
                    info!("sync: {} -> mod_select idx {}", m.name, idx);
                    store_set(controller, "mod_select", idx as u16);
                } else {
                    warn!("sync: {} not found in MOD_CONFIG", m.name);
                }
                if let Some(entry) = MOD_CONFIG.iter().find(|c| *c.name() == m.name) {
                    sync_params(controller, entry.labels(), "mod", &m.parameters);
                }
            }
            "Delay" => {
                if let Some(idx) = DELAY_CONFIG.iter().position(|c| *c.name() == m.name) {
                    info!("sync: {} -> delay_select idx {}", m.name, idx);
                    store_set(controller, "delay_select", idx as u16);
                } else {
                    warn!("sync: {} not found in DELAY_CONFIG", m.name);
                }
                if let Some(entry) = DELAY_CONFIG.iter().find(|c| *c.name() == m.name) {
                    sync_params(controller, entry.labels(), "delay", &m.parameters);
                }
            }
            cat => {
                warn!("sync: unhandled category '{}' for module '{}'", cat, m.name);
            }
        }
    }
}

fn sync_params(
    controller: &Arc<Mutex<Controller>>,
    labels: &std::collections::HashMap<String, String>,
    prefix: &str,
    params: &[pod_usb::ParamValue],
) {
    for (i, pv) in params.iter().enumerate() {
        let key = format!("{}_param{}", prefix, i + 2);
        if !labels.contains_key(&key) {
            continue;
        }
        let value: u16 = match pv {
            pod_usb::ParamValue::Int(v) => *v as u16,
            pod_usb::ParamValue::Float(f) => (f.max(0.0).min(100.0) * 127.0 / 100.0) as u16,
            pod_usb::ParamValue::Bool(true) => 127,
            pod_usb::ParamValue::Bool(false) => 0,
            pod_usb::ParamValue::Raw(_) => 0,
        };
        store_set(controller, &key, value);
    }
}

fn store_set(controller: &Arc<Mutex<Controller>>, name: &str, value: u16) {
    let mut ctrl = controller.lock().unwrap();
    ctrl.set_full(name, value, pod_core::store::Origin::NONE, pod_core::store::Signal::Force);
}
