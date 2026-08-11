//! Turning a control in the UI into an edit on the device.
//!
//! The inverse of [`crate::handler::apply_device_event`], and deliberately the
//! same shape. One rule keeps the two from chasing each other:
//!
//! > **A value the user set carries `Origin::UI`; a value the device reported
//! > carries `Origin::MIDI`.** Only the first is sent.
//!
//! That is not a convention invented here — `generic::midi_cc_in_handler` marks
//! hardware-origin values the same way for every other device pod-ui supports,
//! and `module.rs`'s widgets already store with `StoreOrigin::UI`. So the
//! filter below is the whole of the loop protection: an edit we send comes back
//! on x2, is applied as `MIDI`, and stops there.
//!
//! What the origin split does *not* solve is timing. The echo arrives a few
//! milliseconds late, by which point a dragging user has moved on, and applying
//! it drags the control backwards. [`crate::handler::mark_written`] suppresses
//! exactly the echoes of parameters we just wrote, for exactly as long as that
//! takes.
//!
//! # Addressing
//!
//! Three numbers name a parameter and none of them are the same:
//!
//! | | what it counts |
//! |---|---|
//! | UI position | where the block sits in the chain row, 1-based |
//! | device slot | the block's index in the preset's chain array (key 98) |
//! | parameter index | the position **within one of two lists** (keys 28, 29) |
//!
//! The first two coincide in every preset seen so far, but that is a property
//! of those presets, so the mapping is read from the preset itself
//! (`handler::device_slot`). The third is the subtle one: a block's `@`-prefixed
//! parameters are numbered separately from its ordinary ones, both from zero, so
//! a cab's Distance and its mic type are *both* "index 0" and only key 29 tells
//! them apart. [`crate::model::ParamSpec::wire_index`] does that conversion and
//! is tested as the exact inverse of the one the read path uses.

use std::sync::{Arc, Mutex};

use log::*;
use pod_core::controller::*;
use pod_core::store::{Origin, Store};
use pod_gtk::logic::LogicBuilder;
use pod_gtk::prelude::*;

use crate::config::{self, MAX_FX_PARAMS};

/// Send every UI-origin change of a chain block's controls to the device.
///
/// Registers one callback per control rather than one that scans: the callback
/// already knows which position and which parameter it belongs to, so nothing
/// has to be recovered from the control's name at edit time.
pub fn wire_writes(
    controller: Arc<Mutex<Controller>>, objs: &ObjectList, callbacks: &mut Callbacks,
) -> anyhow::Result<()> {
    for slot in 1..=config::CHAIN_SLOTS {
        let prefix = config::slot_prefix(slot);

        for k in 1..=MAX_FX_PARAMS {
            let mut b = LogicBuilder::new(controller.clone(), objs.clone(), callbacks);
            b.on(&format!("{prefix}_param{k}"))
                .from(Origin::UI)
                .run(move |value, ctrl, _| write_param(slot, k, value, ctrl));
        }

        let mut b = LogicBuilder::new(controller.clone(), objs.clone(), callbacks);
        b.on(&format!("{prefix}_enable"))
            .from(Origin::UI)
            .run(move |value, _, _| write_bypass(slot, value > 0));
    }

    Ok(())
}

/// Send one parameter change.
///
/// `ctrl` is **already locked** — `LogicBuilder` callbacks hand over the
/// controller it holds. Nothing here may take that lock again, and nothing here
/// blocks: the edit is handed to the writer thread and this returns.
fn write_param(slot: usize, k: usize, value: u16, ctrl: &mut Controller) {
    let prefix = config::slot_prefix(slot);

    let Some(device_slot) = crate::handler::device_slot(slot) else {
        // The row has ten positions; a preset need not fill them all, and a
        // position the current preset does not use has no block to address.
        debug!("Pod Go: position {slot} is not in the current chain, not writing");
        return;
    };

    let model_index = ctrl.get(&format!("{prefix}_select")).unwrap_or(0) as usize;
    let Some(model) = config::ALL_MODELS.get(model_index) else { return };
    let Some(def) = model.params.param(k - 1) else { return };
    let Some((index, ordinary)) = model.params.wire_index(k - 1) else { return };

    let wire = crate::handler::wire_value(def, value);
    // One line per edit, naming what it addressed and in whose units. A write
    // that lands on the wrong control is a mapping question, and this answers
    // it without having to reason about parameter order from a data file.
    debug!(
        "Pod Go: write position {slot} (block {device_slot}) {} / {} = {value} -> {} {wire:?}",
        model.name,
        def.name,
        if ordinary { "param" } else { "@param" },
    );

    crate::handler::mark_written(device_slot, index, ordinary);
    crate::device::set_param(device_slot, index, ordinary, wire);
}

/// Switch a block on or off.
fn write_bypass(slot: usize, enabled: bool) {
    let Some(device_slot) = crate::handler::device_slot(slot) else {
        debug!("Pod Go: position {slot} is not in the current chain, not writing");
        return;
    };
    debug!("Pod Go: write position {slot} (block {device_slot}) enable = {enabled}");
    // `u8::MAX` is the bypass's stand-in for a parameter index; see
    // `handler::mark_written`.
    crate::handler::mark_written(device_slot, u8::MAX, true);
    crate::device::set_bypass(device_slot, enabled);
}
