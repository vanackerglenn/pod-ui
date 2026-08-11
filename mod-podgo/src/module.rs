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

/// The single panel every chain position shares.
///
/// One set of widgets serves all ten blocks; selecting a position re-points
/// them rather than building ten panels. Everything the re-pointing needs
/// travels together so the closures that do it take one clone instead of nine.
#[derive(Clone)]
struct Panel {
    pbox: gtk::Box,
    /// The kind of block. Changing it lists that category's models and nothing
    /// more — see [`Panel::list_models`].
    category: gtk::ComboBoxText,
    model: gtk::ComboBoxText,
    enable: gtk::CheckButton,
    title: gtk::Label,
    /// Which [`config::ALL_MODELS`] entry each row of the model combo names.
    /// The model combo holds one category at a time, so its own row numbers
    /// mean nothing on their own.
    listed: Rc<RefCell<Vec<usize>>>,
    setters: Rc<RefCell<Vec<Option<Setter>>>>,
    /// Set while the code, rather than the user, is moving a widget. Every
    /// handler below returns early on it — otherwise re-pointing the panel at a
    /// block would read as editing it.
    updating: Rc<Cell<bool>>,
    focus: Focus,
}

impl Panel {
    /// Fill the model combo with one category's models.
    ///
    /// `select` is an index into [`config::ALL_MODELS`]; when it isn't among
    /// them the combo is left with nothing active, which is how choosing a
    /// category presents itself — the block still holds what it held, and the
    /// user has yet to say what to put there instead.
    ///
    /// The caller must hold `updating`: this moves widgets.
    fn list_models(&self, category: &str, select: Option<usize>) {
        let listed = config::models_in_category(category);
        self.model.remove_all();
        for &i in &listed {
            self.model.append_text(&config::ALL_MODELS[i].name);
        }
        let active = select.and_then(|s| listed.iter().position(|&i| i == s));
        self.model.set_active(active.map(|i| i as u32));
        self.model.set_sensitive(!listed.is_empty());
        *self.listed.borrow_mut() = listed;
    }
}

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
    let panel = Panel {
        pbox: objs.ref_by_name::<gtk::Box>("block_params")?,
        category: objs.ref_by_name::<gtk::ComboBoxText>("block_category")?,
        model: objs.ref_by_name::<gtk::ComboBoxText>("block_model")?,
        enable: objs.ref_by_name::<gtk::CheckButton>("block_enable")?,
        title: objs.ref_by_name::<gtk::Label>("block_title")?,
        listed: Rc::new(RefCell::new(vec![])),
        setters: Rc::new(RefCell::new(vec![])),
        updating: Rc::new(Cell::new(false)),
        focus: Rc::new(RefCell::new(config::slot_prefix(1))),
    };
    let buttons: Rc<RefCell<Vec<(String, gtk::ToggleButton)>>> = Rc::new(RefCell::new(vec![]));

    // The categories never change, so the combo is filled once.
    panel.updating.set(true);
    for c in config::CATEGORIES.iter() {
        panel.category.append_text(c);
    }
    panel.updating.set(false);

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
        let panel = panel.clone();
        let buttons = buttons.clone();
        button.connect_clicked(move |b| {
            if panel.updating.get() {
                return;
            }
            if !b.is_active() {
                // Re-clicking the active block keeps it selected.
                panel.updating.set(true);
                b.set_active(true);
                panel.updating.set(false);
                return;
            }
            *panel.focus.borrow_mut() = prefix.clone();
            let ctrl = controller.lock().unwrap();
            show_block(&prefix, &ctrl, &controller, &panel);
            drop(ctrl);
            // Untoggle the others.
            panel.updating.set(true);
            for (p, other) in buttons.borrow().iter() {
                other.set_active(*p == prefix);
            }
            panel.updating.set(false);
        });
    }

    // --- category combo -> which models the model combo offers --------------
    //
    // Deliberately does *not* change the block. Picking "Delay" says what kind
    // of thing you are looking for, not that any particular delay should be
    // loaded — and since every model change costs the block's settings, one
    // triggered by browsing would be an expensive surprise. The block changes
    // when a model is chosen, below.
    {
        let panel_cb = panel.clone();
        panel.category.connect_changed(move |c| {
            if panel_cb.updating.get() {
                return;
            }
            let Some(category) = c.active().and_then(|i| config::CATEGORIES.get(i as usize))
            else {
                return;
            };
            panel_cb.updating.set(true);
            panel_cb.list_models(category, None);
            panel_cb.updating.set(false);
        });
    }

    // --- model combo -> the focused block's select -------------------------
    {
        let (controller, panel_cb) = (controller.clone(), panel.clone());
        panel.model.connect_changed(move |c| {
            if panel_cb.updating.get() {
                return;
            }
            let Some(row) = c.active() else { return };
            // Copied out before the controller is touched: setting the value
            // runs the callbacks that rebuild this very list.
            let index = panel_cb.listed.borrow().get(row as usize).copied();
            let Some(index) = index else { return };
            let name = format!("{}_select", panel_cb.focus.borrow());
            controller.lock().unwrap().set(&name, index as u16, StoreOrigin::UI);
        });
    }

    // --- enable switch -> the focused block's enable ------------------------
    {
        let (controller, panel_cb) = (controller.clone(), panel.clone());
        panel.enable.connect_toggled(move |c| {
            if panel_cb.updating.get() {
                return;
            }
            let name = format!("{}_enable", panel_cb.focus.borrow());
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

        let panel = panel.clone();
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
            if *panel.focus.borrow() == prefix_cb {
                show_block(&prefix_cb, ctrl, &arc, &panel);
            }
        });
    }

    // Param values: push into the widget only while that block is on screen.
    for (prefix, _) in chain_slots() {
        for k in 1..=MAX_FX_PARAMS {
            let panel = panel.clone();
            let prefix_cb = prefix.clone();
            let mut b = LogicBuilder::new(controller.clone(), objs.clone(), callbacks);
            b.on(&format!("{prefix}_param{k}")).run(move |value, _, _| {
                if *panel.focus.borrow() != prefix_cb {
                    return;
                }
                if let Some(Some(set)) = panel.setters.borrow().get(k - 1) {
                    set(value);
                }
            });
        }
    }


    // Enable state of the focused block.
    for (prefix, _) in chain_slots() {
        let panel = panel.clone();
        let prefix_cb = prefix.clone();
        let mut b = LogicBuilder::new(controller.clone(), objs.clone(), callbacks);
        b.on(&format!("{prefix}_enable")).run(move |value, _, _| {
            if *panel.focus.borrow() != prefix_cb {
                return;
            }
            panel.updating.set(true);
            panel.enable.set_active(value > 0);
            panel.updating.set(false);
        });
    }

    // Show the first block to start with.
    {
        let ctrl = controller.lock().unwrap();
        show_block(&config::slot_prefix(1), &ctrl, &controller, &panel);
    }
    if let Some((_, b)) = buttons.borrow().first() {
        panel.updating.set(true);
        b.set_active(true);
        panel.updating.set(false);
    }

    Ok(())
}

/// Point the shared panel at `prefix`: show the block's category and model,
/// mirror its enable state, and rebuild the param widgets from the selected
/// model's spec.
///
/// The category is view state only — no controller value holds it. It is
/// whatever the block's current model belongs to, so a preset load, a change
/// made on the pedal and a click on another position all set it without
/// anything extra to keep in step.
///
/// `ctrl` is an already-locked controller (LogicBuilder callbacks hand one
/// over); `arc` is for the new widgets' own edit handlers. Never re-lock `arc`
/// here — that deadlocks.
fn show_block(prefix: &str, ctrl: &Controller, arc: &Arc<Mutex<Controller>>, panel: &Panel) {
    let models: &[config::FxModel] = &config::ALL_MODELS;
    let selected = ctrl.get(&format!("{prefix}_select")).unwrap_or(0) as usize;

    let slot = prefix.trim_start_matches("slot");
    let what = models.get(selected).map(|m| m.name.as_str()).unwrap_or(config::EMPTY_MODEL);
    panel.title.set_text(&format!("Position {slot} — {what}"));

    panel.updating.set(true);
    // An unfilled position belongs to no category, so both combos come up
    // blank: there is nothing to show and everything to choose from.
    let category = models.get(selected).map(|m| m.category).unwrap_or("");
    let at = config::CATEGORIES.iter().position(|c| *c == category);
    panel.category.set_active(at.map(|i| i as u32));
    panel.list_models(category, Some(selected));

    let enable_name = format!("{prefix}_enable");
    match ctrl.get_config(&enable_name) {
        Some(_) => {
            panel.enable.set_sensitive(true);
            panel.enable.set_active(ctrl.get(&enable_name).unwrap_or(0) > 0);
        }
        None => {
            // Blocks that can't be bypassed (Cab, Volume) have no enable control.
            panel.enable.set_sensitive(false);
            panel.enable.set_active(true);
        }
    }
    panel.updating.set(false);

    // Volume and FX Loop are mostly hardware — the pedal position, a routing
    // send — but the device does expose their few params (Volume: Pedal +
    // Taper; FX Loop: Send, Return, Mix), and POD Go Edit shows them, so they
    // get the same panel as everything else.
    let empty = crate::model::ParamSpec::default();
    let spec = models.get(selected).map(|m| &m.params).unwrap_or(&empty);
    *panel.setters.borrow_mut() = rebuild_params(prefix, &panel.pbox, spec, arc, ctrl);
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
        for widget in [
            "chain_row", "block_params", "block_category", "block_model", "block_enable",
            "block_title",
        ] {
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

    /// Whatever a block currently holds can be found in its own category's
    /// list.
    ///
    /// This is the contract between the two combos: `show_block` sets the
    /// category from the model and then asks `list_models` to select that model
    /// within it. A model whose category doesn't list it would leave the model
    /// combo blank on a block that plainly holds something — and picking any
    /// entry to make the blank go away would change the patch.
    #[test]
    fn every_model_is_selectable_within_its_own_category() {
        for (i, m) in config::ALL_MODELS.iter().enumerate().skip(1) {
            let listed = config::models_in_category(m.category);
            assert!(
                listed.contains(&i),
                "{} / {} is not in its own category's list",
                m.category, m.name
            );
        }
        // And the empty entry belongs to none of them, so an unfilled position
        // shows both combos blank rather than pretending to hold something.
        assert!(
            config::CATEGORIES.iter().all(|c| !config::models_in_category(c).contains(&0)),
            "(empty) must not appear in a category"
        );
    }

    /// A real patch's blocks each land in a category the panel offers.
    #[test]
    fn a_captured_patch_shows_a_category_for_every_position() {
        use std::sync::{Arc, Mutex};
        use pod_core::store::Store;

        let controller = Arc::new(Mutex::new(Controller::new(config::CONFIG.controls.clone())));
        let data = include_bytes!("../tests/fixtures/a30-fawn-brt.preset.bin");
        let preset = crate::preset_parser::parse_preset_data(data);
        crate::handler::sync_controller_from_preset(&controller, &preset);

        let ctrl = controller.lock().unwrap();
        let mut seen: Vec<&str> = vec![];
        for slot in 1..=config::CHAIN_SLOTS {
            let i = ctrl.get(&format!("{}_select", config::slot_prefix(slot))).unwrap() as usize;
            let m = &config::ALL_MODELS[i];
            assert!(
                config::CATEGORIES.contains(&m.category),
                "position {slot} ({}) has category {:?}, which the combo never offers",
                m.name, m.category
            );
            seen.push(m.category);
        }
        // The patch is one of each fixed block plus four effects, so the
        // categories are a real spread rather than one repeated.
        assert!(seen.contains(&"Amp") && seen.contains(&"Cab") && seen.contains(&"EQ"));
    }
}
