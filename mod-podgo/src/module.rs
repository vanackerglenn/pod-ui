use std::sync::{Arc, Mutex};
use pod_core::edit::EditBuffer;
use pod_core::model::Config;
use pod_gtk::prelude::*;
use gtk::{Builder, Widget};
use pod_core::handler::BoxedHandler;
use pod_core::controller::Controller;
use pod_gtk::logic::LogicBuilder;
use pod_mod_pod2::wiring::{wire_name_change, wire_14bit};

use crate::config;
use crate::handler::PodGoHandler;
use crate::model::{ConfigAccess, StompConfig, ModConfig, DelayConfig};
use crate::config::{STOMP_CONFIG, MOD_CONFIG, DELAY_CONFIG};

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
            init_combo(&ctrl, &self.objects, "reverb_select", &config.effects, |eff| eff.name.as_str())?;
            init_combo(&ctrl, &self.objects, "delay_select", &*DELAY_CONFIG, |c| c.name.as_str())?;
            init_combo(&ctrl, &self.objects, "mod_select", &*MOD_CONFIG, |c| c.name.as_str())?;
            init_combo(&ctrl, &self.objects, "stomp_select", &*STOMP_CONFIG, |c| c.name.as_str())?;
            init_combo(&ctrl, &self.objects, "wah_select", &crate::config::WAH_MODELS, |s| s.as_str())?;
        }

        pod_gtk::wire(controller.clone(), &self.objects, callbacks)?;

        wire_stomp_select(&*STOMP_CONFIG, controller.clone(), &self.objects, callbacks)?;
        wire_mod_select(&*MOD_CONFIG, controller.clone(), &self.objects, callbacks)?;
        wire_delay_select(&*DELAY_CONFIG, controller.clone(), &self.objects, callbacks)?;

        wire_14bit(controller.clone(), &self.objects, callbacks,
                   "mod_speed", "mod_speed:msb", "mod_speed:lsb", true)?;
        wire_14bit(controller.clone(), &self.objects, callbacks,
                   "delay_time", "delay_time:msb", "delay_time:lsb", true)?;

        wire_name_change(edit, config, &self.objects, callbacks)?;

        Ok(())
    }

    fn init(&self, _edit: Arc<Mutex<EditBuffer>>) -> anyhow::Result<()> {
        Ok(())
    }
}

fn wire_dynamic_select<T: ConfigAccess>(
    select_name: &str, configs: &'static [T],
    controller: Arc<Mutex<Controller>>, objs: &ObjectList, callbacks: &mut Callbacks,
) -> anyhow::Result<()> {
    let param_names: std::collections::HashSet<&String> = configs.iter()
        .flat_map(|c| c.labels().keys())
        .collect();

    let mut builder = LogicBuilder::new(controller, objs.clone(), callbacks);
    let objs = objs.clone();
    builder
        .on(select_name)
        .run(move |value, _, _| {
            let config = &configs[value as usize];

            for param in param_names.iter() {
                let label_name = format!("{}_label", param);
                match objs.ref_by_name::<gtk::Label>(&label_name) {
                    Ok(label) => {
                        if let Some(text) = config.labels().get(param.as_str()) {
                            label.set_text(text);
                            label.show();
                        } else {
                            label.hide();
                        }
                    }
                    Err(_) => {}
                }
                match objs.ref_by_name::<gtk::Widget>(param) {
                    Ok(widget) => {
                        if config.labels().contains_key(param.as_str()) {
                            widget.show();
                        } else {
                            widget.hide();
                        }
                    }
                    Err(_) => {}
                }
            }
        });

    Ok(())
}

fn wire_stomp_select(
    stomp_config: &'static [StompConfig],
    controller: Arc<Mutex<Controller>>, objs: &ObjectList, callbacks: &mut Callbacks,
) -> anyhow::Result<()> {
    wire_dynamic_select("stomp_select", stomp_config, controller, objs, callbacks)
}

fn wire_mod_select(
    mod_config: &'static [ModConfig],
    controller: Arc<Mutex<Controller>>, objs: &ObjectList, callbacks: &mut Callbacks,
) -> anyhow::Result<()> {
    wire_dynamic_select("mod_select", mod_config, controller, objs, callbacks)
}

fn wire_delay_select(
    delay_config: &'static [DelayConfig],
    controller: Arc<Mutex<Controller>>, objs: &ObjectList, callbacks: &mut Callbacks,
) -> anyhow::Result<()> {
    wire_dynamic_select("delay_select", delay_config, controller, objs, callbacks)
}

pub fn module() -> impl Module {
    PodGoModule
}
