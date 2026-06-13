use std::rc::Rc;
use std::cell::Cell;
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
use crate::config::{FX_MODELS, MAX_FX_PARAMS};
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
        {
            let ctrl = controller.lock().unwrap();
            init_combo(&ctrl, &self.objects, "amp_select", &config.amp_models, |amp| amp.name.as_str())?;
            init_combo(&ctrl, &self.objects, "cab_select", &config.cab_models, |s| s.as_str())?;
            init_combo(&ctrl, &self.objects, "wah_select", &crate::config::WAH_MODELS, |s| s.as_str())?;
            init_combo(&ctrl, &self.objects, "preset_eq_select", &crate::config::EQ_MODELS, |s| s.as_str())?;
            // The four generic FX slots all share the full FX model catalog.
            for n in 1..=4 {
                init_combo(&ctrl, &self.objects, &format!("fx{}_select", n),
                           &*FX_MODELS, |m| m.name.as_str())?;
            }
        }

        pod_gtk::wire(controller.clone(), &self.objects, callbacks)?;

        // Each FX slot builds its param widgets dynamically to match the
        // selected model (count + widget type per the model's ParamSpec).
        for n in 1..=4 {
            wire_fx_slot(n, controller.clone(), &self.objects, callbacks)?;
        }

        wire_name_change(edit, config, &self.objects, callbacks)?;

        Ok(())
    }

    fn init(&self, _edit: Arc<Mutex<EditBuffer>>) -> anyhow::Result<()> {
        Ok(())
    }
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
                    controller.lock().unwrap().set(&name, if c.is_active() { 127 } else { 0 }, StoreOrigin::UI);
                });
            }
            row.pack_start(&check, true, true, 0);
            let upd = updating.clone();
            Rc::new(move |v: u16| { upd.set(true); check.set_active(v > 63); upd.set(false); })
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
        // numeric kinds (percent/hz/db/ms/time/semitones/int/enum-without-options/unknown)
        _ => {
            let adj = gtk::Adjustment::new(0.0, 0.0, 127.0, 1.0, 8.0, 0.0);
            let scale = gtk::Scale::new(Orientation::Horizontal, Some(&adj));
            scale.set_hexpand(true);
            scale.set_value_pos(gtk::PositionType::Right);
            scale.set_digits(0);
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
    slot: usize, pbox: &gtk::Box, spec: &crate::model::ParamSpec,
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
        let ctrl_name = format!("fx{}_param{}", slot, i + 1);
        let (row, setter) = build_param_row(def, &ctrl_name, arc);
        pbox.add(&row);
        setters.push(Some(setter));
    }
    pbox.show_all();
    // seed widgets from the already-locked controller (guarded setters won't
    // echo back, so no re-lock happens via the value-changed signal)
    for (i, setter) in setters.iter().enumerate() {
        if let Some(set) = setter {
            let v = ctrl.get(&format!("fx{}_param{}", slot, i + 1)).unwrap_or(0);
            set(v);
        }
    }
    setters
}

/// Wire one FX slot: on model-select, rebuild its param widgets to match the
/// model; per-param callbacks keep the widgets in sync with controller values.
fn wire_fx_slot(
    slot: usize,
    controller: Arc<Mutex<Controller>>, objs: &ObjectList, callbacks: &mut Callbacks,
) -> anyhow::Result<()> {
    let pbox = objs.ref_by_name::<gtk::Box>(&format!("fx{}_params", slot))?;
    let setters: Rc<std::cell::RefCell<Vec<Option<Setter>>>> = Rc::new(std::cell::RefCell::new(vec![]));

    // controller -> widget: one callback per param position.
    for k in 1..=MAX_FX_PARAMS {
        let setters = setters.clone();
        let mut b = LogicBuilder::new(controller.clone(), objs.clone(), callbacks);
        b.on(&format!("fx{}_param{}", slot, k)).run(move |value, _, _| {
            if let Some(Some(set)) = setters.borrow().get(k - 1) {
                set(value);
            }
        });
    }

    // model select -> rebuild the param widgets for that model. The callback
    // hands us the already-locked controller (`ctrl`); `arc` is for the new
    // widgets' edit-time handlers.
    let pbox2 = pbox.clone();
    let arc = controller.clone();
    let mut b = LogicBuilder::new(controller, objs.clone(), callbacks);
    b.on(&format!("fx{}_select", slot)).run(move |value, ctrl, _| {
        let Some(model) = FX_MODELS.get(value as usize) else { return; };
        *setters.borrow_mut() = rebuild_params(slot, &pbox2, &model.params, &arc, ctrl);
    });

    Ok(())
}

pub fn module() -> impl Module {
    PodGoModule
}
