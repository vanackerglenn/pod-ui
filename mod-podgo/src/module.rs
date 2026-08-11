use std::rc::Rc;
use std::cell::{Cell, RefCell};
use std::sync::{Arc, Mutex};
use pod_core::edit::EditBuffer;
use pod_core::model::Config;
use pod_gtk::prelude::*;
use gtk::{Builder, Widget, Orientation};
use pod_core::handler::BoxedHandler;
use pod_core::controller::*;
use pod_gtk::logic::LogicBuilder;
use pod_mod_pod2::wiring::wire_name_change;

use crate::config;
use crate::handler::PodGoHandler;
use crate::config::MAX_FX_PARAMS;
use crate::model::{ParamDef, ParamKind};

/// Setter that pushes a controller value into a dynamically-built param widget
/// (signal-guarded so it doesn't echo back to the controller).
type Setter = Rc<dyn Fn(u16)>;

pub struct PodGoModule;

impl Module for PodGoModule {
    fn config(&self) -> Box<[Config]> {
        vec![config::CONFIG.clone()].into_boxed_slice()
    }

    fn init(&self, config: &'static Config) -> Box<dyn Interface> {
        Box::new(PodGoInterface::new(config))
    }

    fn handler(&self, _config: &'static Config) -> BoxedHandler {
        Box::new(PodGoHandler)
    }
}

struct PodGoInterface {
    config: &'static Config,
    widget: Widget,
    objects: ObjectList,
}

impl PodGoInterface {
    fn new(config: &'static Config) -> Self {
        let builder = Builder::from_string(include_str!("pod-go.glade"));
        let objects = ObjectList::new(&builder);

        let widow: gtk::Window = builder.object("app_win").unwrap();
        let widget = widow.child().unwrap();
        widow.remove(&widget);

        let objects = &objects + &ObjectList::from_widget(&widget);

        Self { config, widget, objects }
    }
}



impl Interface for PodGoInterface {
    fn widget(&self) -> Widget {
        self.widget.clone()
    }

    fn objects(&self) -> ObjectList {
        self.objects.clone()
    }

    fn wire(&self, edit: Arc<Mutex<EditBuffer>>, callbacks: &mut Callbacks) -> anyhow::Result<()> {
        let config = self.config;
        let controller = edit.lock().unwrap().controller();

        pod_gtk::wire(controller.clone(), &self.objects, callbacks)?;
        wire_chain(controller.clone(), &self.objects, callbacks)?;
        // Registered after the chain so the UI is fully built before anything
        // can be sent; the two are independent otherwise — `wire_chain` moves
        // values between widgets and the controller, this moves them from the
        // controller to the device.
        crate::live::wire_writes(controller.clone(), &self.objects, callbacks)?;
        wire_name_change(edit, config, &self.objects, callbacks)?;

        Ok(())
    }

    fn init(&self, _edit: Arc<Mutex<EditBuffer>>) -> anyhow::Result<()> {
        Ok(())
    }
}

/// Which block the params panel is currently showing.
type Focus = Rc<RefCell<String>>;

/// The chain, as the ten positions the device runs in order.
///
/// Position *is* identity here: `slot3` is the third block, whatever kind of
/// block that happens to be. So the row never needs re-sorting and no block can
/// be mis-placed — which is what keying the UI by category used to cause.
fn chain_slots() -> impl Iterator<Item = (String, String)> {
    (1..=config::CHAIN_SLOTS).map(|n| (config::slot_prefix(n), format!("{n}")))
}

/// Build the chain row and the single params panel beneath it.
///
/// One panel serves every block: selecting a chain button re-points the model
/// combo, the enable switch and the param widgets at that block's controls.
/// The controller still holds all ten blocks' values at once — only the view is
/// shared.
fn wire_chain(
    controller: Arc<Mutex<Controller>>, objs: &ObjectList, callbacks: &mut Callbacks,
) -> anyhow::Result<()> {
    let row = objs.ref_by_name::<gtk::Box>("chain_row")?;
    let pbox = objs.ref_by_name::<gtk::Box>("block_params")?;
    let combo = objs.ref_by_name::<gtk::ComboBoxText>("block_model")?;
    let enable = objs.ref_by_name::<gtk::CheckButton>("block_enable")?;
    let title = objs.ref_by_name::<gtk::Label>("block_title")?;

    let focus: Focus = Rc::new(RefCell::new(config::slot_prefix(1)));
    let setters: Rc<RefCell<Vec<Option<Setter>>>> = Rc::new(RefCell::new(vec![]));
    let updating = Rc::new(Cell::new(false));
    let buttons: Rc<RefCell<Vec<(String, gtk::ToggleButton)>>> = Rc::new(RefCell::new(vec![]));

    // --- the chain row -----------------------------------------------------
    for (prefix, label) in chain_slots() {
        let b = gtk::ToggleButton::new();
        let bl = gtk::Label::new(Some(&label));
        bl.set_ellipsize(gtk::pango::EllipsizeMode::End);
        bl.set_max_width_chars(14);
        b.add(&bl);
        b.set_tooltip_text(Some(&label));
        row.add(&b);
        buttons.borrow_mut().push((prefix, b));
    }
    row.show_all();

    // Clicking a block focuses it. `show_block` does the actual repointing.
    for (prefix, button) in buttons.borrow().iter() {
        let (prefix, controller) = (prefix.clone(), controller.clone());
        let (focus, setters, updating) = (focus.clone(), setters.clone(), updating.clone());
        let (pbox, combo, enable, title) =
            (pbox.clone(), combo.clone(), enable.clone(), title.clone());
        let buttons = buttons.clone();
        button.connect_clicked(move |b| {
            if updating.get() {
                return;
            }
            if !b.is_active() {
                // Re-clicking the active block keeps it selected.
                updating.set(true);
                b.set_active(true);
                updating.set(false);
                return;
            }
            *focus.borrow_mut() = prefix.clone();
            let ctrl = controller.lock().unwrap();
            show_block(&prefix, &ctrl, &controller, &pbox, &combo, &enable, &title,
                       &setters, &updating);
            drop(ctrl);
            // Untoggle the others.
            updating.set(true);
            for (p, other) in buttons.borrow().iter() {
                other.set_active(*p == prefix);
            }
            updating.set(false);
        });
    }

    // --- model combo -> the focused block's select -------------------------
    {
        let (controller, focus, updating) = (controller.clone(), focus.clone(), updating.clone());
        combo.connect_changed(move |c| {
            if updating.get() {
                return;
            }
            if let Some(i) = c.active() {
                let name = format!("{}_select", focus.borrow());
                controller.lock().unwrap().set(&name, i as u16, StoreOrigin::UI);
            }
        });
    }

    // --- enable switch -> the focused block's enable ------------------------
    {
        let (controller, focus, updating) = (controller.clone(), focus.clone(), updating.clone());
        enable.connect_toggled(move |c| {
            if updating.get() {
                return;
            }
            let name = format!("{}_enable", focus.borrow());
            let mut ctrl = controller.lock().unwrap();
            if ctrl.get_config(&name).is_some() {
                ctrl.set(&name, if c.is_active() { 1 } else { 0 }, StoreOrigin::UI);
            }
        });
    }

    // --- controller -> UI ---------------------------------------------------
    // A block's model changing updates its chain button's label always, and
    // rebuilds the params panel when that block is the one on screen.
    for (prefix, label) in chain_slots() {
        let button = buttons
            .borrow()
            .iter()
            .find(|(p, _)| *p == prefix)
            .map(|(_, b)| b.clone())
            .unwrap();
        let (prefix_cb, label_cb) = (prefix.clone(), label.clone());

        let (focus, setters, updating) = (focus.clone(), setters.clone(), updating.clone());
        let (pbox, combo, enable, title) =
            (pbox.clone(), combo.clone(), enable.clone(), title.clone());
        let arc = controller.clone();
        let mut b = LogicBuilder::new(controller.clone(), objs.clone(), callbacks);
        b.on(&format!("{prefix}_select")).run(move |value, ctrl, _| {
            let name = config::ALL_MODELS.get(value as usize).map(|m| m.name.as_str());
            if let Some(l) = button.child().and_then(|c| c.downcast::<gtk::Label>().ok()) {
                let model = name.unwrap_or(config::EMPTY_MODEL);
                // The button shows the model, and "(empty)" when the position
                // holds nothing. Which position it is stays in the tooltip.
                l.set_text(model);
                l.set_tooltip_text(Some(&format!("Position {label_cb}: {model}")));
                l.set_sensitive(model != config::EMPTY_MODEL);
            }
            if *focus.borrow() == prefix_cb {
                show_block(&prefix_cb, ctrl, &arc, &pbox, &combo, &enable, &title,
                           &setters, &updating);
            }
        });
    }

    // Param values: push into the widget only while that block is on screen.
    for (prefix, _) in chain_slots() {
        for k in 1..=MAX_FX_PARAMS {
            let (focus, setters) = (focus.clone(), setters.clone());
            let prefix_cb = prefix.clone();
            let mut b = LogicBuilder::new(controller.clone(), objs.clone(), callbacks);
            b.on(&format!("{prefix}_param{k}")).run(move |value, _, _| {
                if *focus.borrow() != prefix_cb {
                    return;
                }
                if let Some(Some(set)) = setters.borrow().get(k - 1) {
                    set(value);
                }
            });
        }
    }


    // Enable state of the focused block.
    for (prefix, _) in chain_slots() {
        let (focus, updating, enable) = (focus.clone(), updating.clone(), enable.clone());
        let prefix_cb = prefix.clone();
        let mut b = LogicBuilder::new(controller.clone(), objs.clone(), callbacks);
        b.on(&format!("{prefix}_enable")).run(move |value, _, _| {
            if *focus.borrow() != prefix_cb {
                return;
            }
            updating.set(true);
            enable.set_active(value > 0);
            updating.set(false);
        });
    }

    // Show the first block to start with.
    {
        let ctrl = controller.lock().unwrap();
        show_block(&config::slot_prefix(1), &ctrl, &controller, &pbox, &combo, &enable,
                   &title, &setters, &updating);
    }
    if let Some((_, b)) = buttons.borrow().first() {
        updating.set(true);
        b.set_active(true);
        updating.set(false);
    }

    Ok(())
}

/// Point the shared panel at `prefix`: fill the model combo with that block's
/// models, select the current one, mirror its enable state, and rebuild the
/// param widgets from the selected model's spec.
///
/// `ctrl` is an already-locked controller (LogicBuilder callbacks hand one
/// over); `arc` is for the new widgets' own edit handlers. Never re-lock `arc`
/// here — that deadlocks.
#[allow(clippy::too_many_arguments)]
fn show_block(
    prefix: &str,
    ctrl: &Controller,
    arc: &Arc<Mutex<Controller>>,
    pbox: &gtk::Box,
    combo: &gtk::ComboBoxText,
    enable: &gtk::CheckButton,
    title: &gtk::Label,
    setters: &Rc<RefCell<Vec<Option<Setter>>>>,
    updating: &Rc<Cell<bool>>,
) {
    let models: &[config::FxModel] = &config::ALL_MODELS;
    let selected = ctrl.get(&format!("{prefix}_select")).unwrap_or(0) as usize;

    let slot = prefix.trim_start_matches("slot");
    let what = models.get(selected).map(|m| m.name.as_str()).unwrap_or(config::EMPTY_MODEL);
    title.set_text(&format!("Position {slot} — {what}"));

    updating.set(true);
    combo.remove_all();
    for m in models {
        combo.append_text(&m.name);
    }
    if selected < models.len() {
        combo.set_active(Some(selected as u32));
    }
    // A block with a single fixed model has nothing to choose.
    combo.set_sensitive(models.len() > 1);

    let enable_name = format!("{prefix}_enable");
    match ctrl.get_config(&enable_name) {
        Some(_) => {
            enable.set_sensitive(true);
            enable.set_active(ctrl.get(&enable_name).unwrap_or(0) > 0);
        }
        None => {
            // Blocks that can't be bypassed (Cab, Volume) have no enable control.
            enable.set_sensitive(false);
            enable.set_active(true);
        }
    }
    updating.set(false);

    // Volume and FX Loop are mostly hardware — the pedal position, a routing
    // send — but the device does expose their few params (Volume: Pedal +
    // Taper; FX Loop: Send, Return, Mix), and POD Go Edit shows them, so they
    // get the same panel as everything else.
    let empty = crate::model::ParamSpec::default();
    let spec = models.get(selected).map(|m| &m.params).unwrap_or(&empty);
    *setters.borrow_mut() = rebuild_params(prefix, pbox, spec, arc, ctrl);
}

/// Build one param row (label + a widget chosen by the param's kind) bound to
/// the `ctrl_name` controller value. Returns the row widget and a guarded
/// setter that pushes a controller value into the widget.
fn build_param_row(
    def: &ParamDef, ctrl_name: &str, controller: &Arc<Mutex<Controller>>,
) -> (Widget, Setter) {
    let row = gtk::Box::new(Orientation::Horizontal, 8);
    let label = gtk::Label::new(Some(&def.name));
    label.set_width_request(90);
    label.set_xalign(1.0);
    row.pack_start(&label, false, false, 0);

    let updating = Rc::new(Cell::new(false));
    let name = ctrl_name.to_string();

    let setter: Setter = match &def.kind {
        ParamKind::Bool => {
            let check = gtk::CheckButton::new();
            check.set_hexpand(true);
            {
                let controller = controller.clone(); let name = name.clone(); let upd = updating.clone();
                check.connect_toggled(move |c| {
                    if upd.get() { return; }
                    let v = if c.is_active() { config::PARAM_CARRIER } else { 0 };
                    controller.lock().unwrap().set(&name, v, StoreOrigin::UI);
                });
            }
            row.pack_start(&check, true, true, 0);
            let upd = updating.clone();
            let on = config::PARAM_CARRIER / 2;
            Rc::new(move |v: u16| { upd.set(true); check.set_active(v > on); upd.set(false); })
        }
        ParamKind::Enum if !def.options.is_empty() => {
            let combo = gtk::ComboBoxText::new();
            combo.set_hexpand(true);
            for opt in &def.options { combo.append_text(opt); }
            {
                let controller = controller.clone(); let name = name.clone(); let upd = updating.clone();
                combo.connect_changed(move |c| {
                    if upd.get() { return; }
                    if let Some(i) = c.active() {
                        controller.lock().unwrap().set(&name, i as u16, StoreOrigin::UI);
                    }
                });
            }
            row.pack_start(&combo, true, true, 0);
            let upd = updating.clone();
            Rc::new(move |v: u16| { upd.set(true); combo.set_active(Some(v as u32)); upd.set(false); })
        }
        // Numeric kinds (and enum-without-options) -> slider. The control value
        // is a position in `0..=PARAM_CARRIER`; the *displayed* number is mapped
        // back into the param's own units (`def.min`..`def.max`, e.g. Hz, dB,
        // ms) so the readout matches the device screen rather than showing the
        // raw carrier value.
        _ => {
            let carrier = config::PARAM_CARRIER as f64;
            // One arrow-key press should move the readout by one digit of the
            // precision it is shown at, not by an invisible fraction of the
            // carrier. A `%.0f` percent steps by 1 %, a `%.1f` dB by 0.1 dB.
            let displayable = ((def.max - def.min).abs() * 10f64.powi(def.decimals as i32))
                .max(1.0);
            let step = (carrier / displayable).max(1.0);
            let adj = gtk::Adjustment::new(0.0, 0.0, carrier, step, (step * 10.0).min(carrier), 0.0);
            let scale = gtk::Scale::new(Orientation::Horizontal, Some(&adj));
            scale.set_hexpand(true);
            scale.set_value_pos(gtk::PositionType::Right);
            scale.set_digits(0);
            {
                let (lo, hi) = (def.min, def.max);
                let decimals = def.decimals as usize;
                let unit = def.unit.clone();
                let off_at = def.off_at;
                scale.connect_format_value(move |_, v| {
                    let norm = v / carrier;
                    match off_at {
                        Some(crate::model::Edge::Min) if norm <= 0.0 => return "Off".to_string(),
                        Some(crate::model::Edge::Max) if norm >= 1.0 => return "Off".to_string(),
                        _ => {}
                    }
                    let display = lo + norm * (hi - lo);
                    if unit.is_empty() {
                        format!("{display:.decimals$}")
                    } else {
                        format!("{display:.decimals$} {unit}")
                    }
                });
            }
            {
                let controller = controller.clone(); let name = name.clone(); let upd = updating.clone();
                adj.connect_value_changed(move |a| {
                    if upd.get() { return; }
                    controller.lock().unwrap().set(&name, a.value() as u16, StoreOrigin::UI);
                });
            }
            row.pack_start(&scale, true, true, 0);
            let upd = updating.clone();
            Rc::new(move |v: u16| { upd.set(true); adj.set_value(v as f64); upd.set(false); })
        }
    };

    (row.upcast::<Widget>(), setter)
}

/// Rebuild a slot's param container to match `spec` exactly (one widget per
/// param; positions with an empty name are skipped but keep index alignment).
/// `arc` is captured by the widgets' edit-time handlers; `ctrl` is the
/// already-locked controller used to seed initial values (do NOT re-lock — the
/// callback that calls this already holds the controller lock).
fn rebuild_params(
    prefix: &str, pbox: &gtk::Box, spec: &crate::model::ParamSpec,
    arc: &Arc<Mutex<Controller>>, ctrl: &Controller,
) -> Vec<Option<Setter>> {
    for child in pbox.children() { pbox.remove(&child); }
    let mut setters: Vec<Option<Setter>> = Vec::with_capacity(spec.len());
    for i in 0..spec.len().min(MAX_FX_PARAMS) {
        let def = spec.param(i).unwrap();
        if def.name.is_empty() {
            setters.push(None);
            continue;
        }
        let ctrl_name = format!("{}_param{}", prefix, i + 1);
        let (row, setter) = build_param_row(def, &ctrl_name, arc);
        pbox.add(&row);
        setters.push(Some(setter));
    }
    pbox.show_all();
    // seed widgets from the already-locked controller (guarded setters won't
    // echo back, so no re-lock happens via the value-changed signal)
    for (i, setter) in setters.iter().enumerate() {
        if let Some(set) = setter {
            let v = ctrl.get(&format!("{}_param{}", prefix, i + 1)).unwrap_or(0);
            set(v);
        }
    }
    setters
}

pub fn module() -> impl Module {
    PodGoModule
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every widget `wire_chain` resolves must exist in the glade *with a
    /// `name` property*.
    ///
    /// `ObjectList::ref_by_name` matches on the widget's `name` property, not
    /// the builder `id`, and a missing one is not a compile error — it fails at
    /// startup with "Object not found by name", taking the whole device config
    /// down. This test is the cheap stand-in for launching the GUI.
    #[test]
    fn glade_provides_every_widget_wire_chain_looks_up() {
        let glade = include_str!("pod-go.glade");
        for widget in ["chain_row", "block_params", "block_model", "block_enable", "block_title"] {
            let prop = format!("<property name=\"name\">{widget}</property>");
            assert!(
                glade.contains(&prop),
                "glade has no widget named {widget:?} (an `id=` alone is not enough)"
            );
        }
    }

    /// The row covers exactly the device's chain positions, and every one has
    /// controls declared for it.
    #[test]
    fn the_row_covers_every_chain_position() {
        let slots: Vec<(String, String)> = chain_slots().collect();
        assert_eq!(slots.len(), config::CHAIN_SLOTS);
        for (prefix, _) in &slots {
            assert!(
                config::CONFIG.controls.contains_key(&format!("{prefix}_select")),
                "{prefix} has no select control"
            );
            assert!(
                config::CONFIG.controls.contains_key(&format!("{prefix}_param1")),
                "{prefix} has no param controls"
            );
        }
        assert!(!config::ALL_MODELS.is_empty());
    }
}
