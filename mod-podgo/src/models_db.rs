//! POD Go model + parameter database, loaded from POD Go Edit's own data files.
//!
//! The device sends parameters *positionally* over USB and carries no schema —
//! no names, no ranges, no enum labels (see `usb/docs/podgo-connect-findings.md`).
//! Those all used to be hand-authored in `module_params.toml`. They no longer
//! are: POD Go Edit ships its own model database, and `mod-podgo/data/` is a
//! copy of it. This module is the loader.
//!
//! Nothing here matches on display names. Names are ambiguous — `eq.models`
//! lists two "Parametric"s, `Sweep Echo` is both an HD2 and a DL4 model, and
//! most cab names appear in both `cab.models` and `cabmicirs.models` — and the
//! device names several blocks differently from the files anyway ("Volume
//! Pedal" is `Volume`, "Mono FX Loop" is `FX Loop 1`). The wire id sidesteps
//! all of it.
//!
//! The four inputs, and what each is authoritative for:
//!
//! - `PodGo.sym` — **the model id table**. Its entries are `symbolicID`s and an
//!   entry's *array position is the numeric model id* the device puts in a
//!   block's chain meta. This is what makes lookup exact: `id` →
//!   `PodGo.sym[id]` → `symbolicID` → the `*.models` entry, with no name
//!   matching anywhere. Verified against every block of the captured A30 Fawn
//!   Brt preset: 239→`HD2_WahFasselStereo`, 224→`HD2_VolPanVolStereo`,
//!   119→`HD2_FXLoopMono1`, 94→`HD2_DistTopSecretODMono`, 7→`HD2_AmpA30FawnBrt`,
//!   53→`HD2_Cab2x12BlueBell`, 100→`HD2_CompressorLAStudioCompMono`,
//!   86→`HD2_DelayTransistorTapeStereo`, 210→`HD2_ReverbRoomStereo`,
//!   472→`HD2_EQ_STATIC_ParametricStereo`.
//!
//! - `*.models` — one file per category. 574 models, 5723 params, each with a
//!   `name`, a `valueType`, a DSP `min`/`max`/`default` and a `displayType`.
//!   This is the parameter data.
//! - `PGControls.json` — 210 `displayType` definitions: units, printf-style
//!   format rules, `dspToDisplayScale`, and the label lists for discrete
//!   params. This is the units-and-labels data.
//! - `PGModelCatalog.json` — the POD Go model inventory, grouped into the
//!   categories the device itself shows. This is the "which models exist"
//!   data. (Its Amp and Cab/IR categories are empty — those lists come from
//!   `amp.models` / `preamp.models` / `cab.models` / `cabmicirs.models`.)
//!
//! # Two encodings, one rule
//!
//! A param's `min`/`max` in `*.models` are **DSP** units — what goes on the
//! wire. The display value is derived from them:
//!
//! - if the control declares `minimumValue`/`maximumValue`, the DSP range maps
//!   linearly onto that (pan: DSP `0..1` → display `-100..100`);
//! - otherwise display = DSP × `dspToDisplayScale` (percent ×100, generic_knob
//!   ×10, time_ms ×1000 — seconds to milliseconds).
//!
//! That single rule reproduces the whole per-kind table in
//! `usb/docs/podgo-value-encoding.md`, including why dB and Hz look "native"
//! (their DSP range simply *is* the display range) while percent looks
//! "normalized" (DSP `0..1`, scale 100).
//!
//! # Discrete params are offset by `min`, not 0-based
//!
//! For a discrete param the wire carries the DSP value, so the label index is
//! `value - dsp_min` — **not** `value` as `podgo-value-encoding.md` states. 116
//! of 395 discrete params have a non-zero `min`; every `sync_note` is `1..19`.
//! This is what makes a captured Gray Flanger note-division of `7` read as
//! "1/4 Triplet" (`format[7-1]`) rather than "1/8 Dotted" (`format[7]`).

use std::collections::HashMap;

use once_cell::sync::Lazy;
use serde::Deserialize;
use serde_json::Value as J;

use crate::model::{Edge, ParamDef, ParamKind, ParamSpec, WireType};

// === Embedded data files ===

macro_rules! models_files {
    ($($cat:expr => $file:expr),* $(,)?) => {
        &[$(($cat, include_str!(concat!("../data/", $file)))),*]
    };
}

/// The `*.models` files, each tagged with the pod-ui category its models belong
/// to. `PGModelCatalog.json` supplies categories for the FX models; these tags
/// are the fallback, and the only source for amps and cabs (whose catalog
/// entries are empty placeholders).
static MODELS_FILES: &[(&str, &str)] = models_files![
    // Amp/Preamp and Cab/Cab-IR are near-duplicate name lists for different
    // block types (104 of 106 amp names recur as preamps; 28 of 41 cab names
    // recur as mic'd IR cabs). They get distinct categories so a wire id lands
    // on exactly one of them. The captured A30 Fawn Brt preset confirms the
    // split: its amp block sends 12 values (`HD2_AmpA30FawnBrt`, not the
    // 7-param `HD2_PreampA30FawnBrt`) and its cab block 6 (`HD2_Cab2x12BlueBell`
    // plus `@mic`, not the 8-param `HD2_CabMicIr_2x12BlueBell`).
    "Amp"        => "amp.models",
    "Preamp"     => "preamp.models",
    "Cab"        => "cab.models",
    "Cab/IR"     => "cabmicirs.models",
    "Distortion" => "distortion.models",
    "Dynamic"    => "compressor.models",
    "Dynamic"    => "gate.models",
    "EQ"         => "eq.models",
    "Modulation" => "modulation.models",
    "Delay"      => "delay.models",
    "Reverb"     => "reverb.models",
    "Pitch/Synth" => "pitch-synth.models",
    "Filter"     => "filter.models",
    "Wah"        => "wah.models",
    "Vol/Pan"    => "volumepan.models",
    "Send/Return" => "sendreturn.models",
    "Send/Return" => "io.models",
    "Fixed"      => "fixed.models",
];

static CONTROLS_JSON: &str = include_str!("../data/PGControls.json");
/// The model id table: `PodGo.sym[id]` is the `symbolicID` of the model the
/// device calls `id`. Its array position *is* the wire id.
static SYM_JSON: &str = include_str!("../data/PodGo.sym");
static CATALOG_JSON: &str = include_str!("../data/PGModelCatalog.json");

/// POD Go Edit's category names mapped onto the ones pod-ui already uses
/// (`preset_parser::is_fixed_block_category`, the model dropdowns).
fn pod_ui_category(catalog_name: &str) -> &'static str {
    match catalog_name {
        "Distortion" => "Distortion",
        "Dynamics" => "Dynamic",
        "EQ" => "EQ",
        "Modulation" => "Modulation",
        "Delay" => "Delay",
        "Reverb" => "Reverb",
        "Pitch" => "Pitch/Synth",
        "Filter" => "Filter",
        "Looper" => "Looper",
        "Wah" => "Wah",
        "Volume" => "Vol/Pan",
        "Send/Return" => "Send/Return",
        "Amp" => "Amp",
        "Cab/IR" => "Cab",
        _ => "Unknown",
    }
}

// === Raw JSON shapes ===

#[derive(Deserialize)]
struct RawModel {
    #[serde(rename = "symbolicID")]
    symbolic_id: String,
    name: Option<String>,
    #[serde(default)]
    params: Vec<RawParam>,
}

#[derive(Deserialize)]
struct RawParam {
    #[serde(rename = "symbolicID")]
    symbolic_id: String,
    name: String,
    #[serde(rename = "valueType")]
    value_type: i64,
    #[serde(rename = "displayType")]
    display_type: Option<String>,
    min: Option<J>,
    max: Option<J>,
}

#[derive(Deserialize)]
struct RawControl {
    /// Another control name whose definition to use instead (`cab_high_cut`
    /// aliases `eq_high_cut`).
    alias: Option<String>,
    #[serde(rename = "isDiscrete")]
    is_discrete: Option<bool>,
    #[serde(rename = "dspToDisplayScale")]
    dsp_to_display_scale: Option<f64>,
    #[serde(rename = "dspToDisplayIntegerOffset")]
    integer_offset: Option<f64>,
    #[serde(rename = "minimumValue")]
    minimum_value: Option<f64>,
    #[serde(rename = "maximumValue")]
    maximum_value: Option<f64>,
    /// Either a printf format string, a list of discrete labels, or a list of
    /// piecewise `{lowerBound, upperBound, format, formatUnits}` rules.
    format: Option<J>,
    #[serde(rename = "formatUnits")]
    format_units: Option<J>,
}

/// One entry of `PodGo.sym`. Its index in the file is the model's wire id.
#[derive(Deserialize)]
struct SymEntry {
    symbol: String,
}

#[derive(Deserialize)]
struct RawCatalog {
    categories: Vec<RawCategory>,
}

#[derive(Deserialize)]
struct RawCategory {
    name: String,
    #[serde(default)]
    models: Vec<RawCatalogModel>,
}

#[derive(Deserialize)]
struct RawCatalogModel {
    /// A `symbolicID` for real models; a bare number for the Amp/Cab
    /// placeholder rows.
    id: J,
}

// === Loaded model ===

#[derive(Clone, Debug)]
pub struct Model {
    pub symbolic_id: String,
    pub name: String,
    pub category: &'static str,
    pub spec: ParamSpec,
}

pub struct ModelDb {
    models: Vec<Model>,
    /// Numeric wire model id -> index into `models`. **The primary key**, built
    /// by walking `PodGo.sym`: entry `n` of that file names the model with id
    /// `n`. Exact and unambiguous — no name matching involved.
    by_id: HashMap<u64, usize>,
    /// Normalized display name -> indices into `models`. The fallback, for
    /// blocks whose id isn't in the id table yet. A name can map to several
    /// models (`Sweep Echo` is both the HD2 and the DL4 model).
    by_name: HashMap<String, Vec<usize>>,
}

/// Params the files list but the device does *not* send positionally: block
/// state carried by its own commands, not by the value array.
///
/// Not every `@`-prefixed param qualifies — `@mic` and `@trails` really are
/// positional. The captured A30 Fawn Brt preset settles it: dropping just these
/// two reproduces the device's value count for every block in it (Fassel 5,
/// Top Secret OD 2, 2x12 Blue Bell 6, LA Studio Comp 6, Transistor Tape 11,
/// Room 7, Parametric 12, A30 Fawn Brt 12).
static NON_POSITIONAL: &[&str] = &["@enabled", "@bypassvolume"];

/// Fold a display name to a comparison key: lowercase, alphanumerics only.
/// Absorbs the spacing/casing/punctuation drift between what the device screen
/// shows and what Line 6's files spell (`Color Drive`/`Colordrive`,
/// `Cali Texas Ch1`/`Cali Texas Ch 1`, `63 Spring`/`'63 Spring`).
fn normalize(name: &str) -> String {
    name.chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

impl ModelDb {
    fn load() -> ModelDb {
        let controls: HashMap<String, RawControl> =
            serde_json::from_str(CONTROLS_JSON).unwrap_or_else(|e| {
                log::error!("failed to parse PGControls.json: {e}");
                HashMap::new()
            });

        // symbolicID -> pod-ui category, from the catalog. Authoritative for FX;
        // amps and cabs aren't listed there and fall back to the file tag.
        let mut catalog_category: HashMap<String, &'static str> = HashMap::new();
        match serde_json::from_str::<RawCatalog>(CATALOG_JSON) {
            Ok(cat) => {
                for c in &cat.categories {
                    let pod_cat = pod_ui_category(&c.name);
                    for m in &c.models {
                        if let Some(id) = m.id.as_str() {
                            if id != "None" {
                                catalog_category.insert(id.to_string(), pod_cat);
                            }
                        }
                    }
                }
            }
            Err(e) => log::error!("failed to parse PGModelCatalog.json: {e}"),
        }

        let mut models = Vec::new();
        for (file_category, src) in MODELS_FILES {
            let raw: Vec<RawModel> = match serde_json::from_str(src) {
                Ok(v) => v,
                Err(e) => {
                    log::error!("failed to parse {file_category} models: {e}");
                    continue;
                }
            };
            for m in raw {
                let Some(name) = m.name.filter(|n| !n.is_empty()) else { continue };
                let category = catalog_category
                    .get(&m.symbolic_id)
                    .copied()
                    .unwrap_or(file_category);
                let mut defs: Vec<ParamDef> = m
                    .params
                    .iter()
                    .filter(|p| !NON_POSITIONAL.contains(&p.symbolic_id.as_str()))
                    .map(|p| param_def(p, &controls))
                    .collect();
                specials_last(&mut defs);
                models.push(Model {
                    symbolic_id: m.symbolic_id,
                    name,
                    category,
                    spec: ParamSpec::from_defs(defs),
                });
            }
        }

        let mut by_name: HashMap<String, Vec<usize>> = HashMap::new();
        for (i, m) in models.iter().enumerate() {
            by_name.entry(normalize(&m.name)).or_default().push(i);
        }
        // symbolicID -> index, then the id table itself. `PodGo.sym` is an
        // ordered list of symbols whose position is the model id the device
        // puts in a block's chain meta, so this is a direct join with no
        // name or param-count guessing anywhere.
        let by_symbol: HashMap<&str, usize> = models
            .iter()
            .enumerate()
            .map(|(i, m)| (m.symbolic_id.as_str(), i))
            .collect();
        let mut by_id: HashMap<u64, usize> = HashMap::new();
        match serde_json::from_str::<Vec<SymEntry>>(SYM_JSON) {
            Ok(syms) => {
                for (id, sym) in syms.iter().enumerate() {
                    if let Some(&i) = by_symbol.get(sym.symbol.as_str()) {
                        by_id.insert(id as u64, i);
                    }
                }
            }
            Err(e) => log::error!("failed to parse PodGo.sym: {e}"),
        }

        ModelDb { models, by_id, by_name }
    }

    /// Every `(wire id, model)` pair, ascending by id.
    pub fn entries_by_id(&self) -> impl Iterator<Item = (u64, &Model)> {
        let mut ids: Vec<u64> = self.by_id.keys().copied().collect();
        ids.sort_unstable();
        ids.into_iter().map(move |id| (id, &self.models[self.by_id[&id]]))
    }

    /// The model for a numeric wire id. This is the identifier every preset
    /// block carries, and the only one an Amp or Cab block carries.
    pub fn by_wire_id(&self, id: u64) -> Option<&Model> {
        self.by_id.get(&id).map(|&i| &self.models[i])
    }

    /// Every model matching `name`, in file order.
    pub fn lookup(&self, name: &str) -> Vec<&Model> {
        self.by_name
            .get(&normalize(name))
            .map(|ids| ids.iter().map(|&i| &self.models[i]).collect())
            .unwrap_or_default()
    }

    /// The model for a preset block: by numeric wire id when there is one,
    /// falling back to the display name for ids the table doesn't cover.
    ///
    /// Prefer passing the id. It is unambiguous, and Amp/Cab blocks have no
    /// name to fall back to.
    pub fn resolve(&self, id: Option<u64>, name: &str) -> Option<&Model> {
        if let Some(m) = id.and_then(|id| self.by_wire_id(id)) {
            return Some(m);
        }
        // No id match: fall back to the name. A name shared by two models is
        // reported rather than silently guessed — picking wrong would misalign
        // every positional param index for that block.
        match self.lookup(name).as_slice() {
            [one] => Some(one),
            [] => None,
            many => {
                log::warn!(
                    "POD Go: model name {name:?} matches {} models and no wire id \
                     resolved it; params left unmapped",
                    many.len()
                );
                None
            }
        }
    }

    pub fn models(&self) -> &[Model] {
        &self.models
    }

    /// Model names in a pod-ui category, in the order the files list them.
    pub fn names_in_category(&self, category: &str) -> Vec<&str> {
        self.models
            .iter()
            .filter(|m| m.category == category)
            .map(|m| m.name.as_str())
            .collect()
    }
}

pub static DB: Lazy<ModelDb> = Lazy::new(ModelDb::load);

/// Look through a control's `alias` indirection.
fn resolve_control<'a>(
    name: &str,
    controls: &'a HashMap<String, RawControl>,
) -> Option<&'a RawControl> {
    let c = controls.get(name)?;
    match &c.alias {
        Some(target) => controls.get(target).or(Some(c)),
        None => Some(c),
    }
}

fn as_f64(v: &Option<J>) -> Option<f64> {
    match v.as_ref()? {
        J::Number(n) => n.as_f64(),
        J::Bool(b) => Some(if *b { 1.0 } else { 0.0 }),
        _ => None,
    }
}

/// The discrete labels a control declares, if it declares a plain list of them.
/// A piecewise list of `{lowerBound, ...}` rules is a numeric format, not
/// labels; `integer_slider*` controls declare no labels at all.
fn labels(c: &RawControl) -> Vec<String> {
    let Some(J::Array(items)) = c.format.as_ref() else { return Vec::new() };
    items
        .iter()
        .map(|v| v.as_str().map(str::to_string))
        .collect::<Option<Vec<_>>>()
        .unwrap_or_default()
}

/// The piecewise format rules, widest-value-last.
fn format_rules(c: &RawControl) -> Vec<(&J, &J)> {
    let Some(J::Array(items)) = c.format.as_ref() else { return Vec::new() };
    items
        .iter()
        .filter_map(|v| {
            let o = v.as_object()?;
            Some((o.get("format")?, o.get("formatUnits").unwrap_or(&J::Null)))
        })
        .collect()
}

/// Decimal places from a printf spec: `%.1f` -> 1, `%+.2f` -> 2, `%.0f` -> 0.
fn decimals_from(spec: &str) -> Option<u8> {
    let after_dot = spec.split_once('.')?.1;
    let digits: String = after_dot.chars().take_while(|c| c.is_ascii_digit()).collect();
    digits.parse().ok()
}

/// The unit suffix trailing a printf spec: `"%.0f Hz"` -> `Hz`, `"%.0f %%"` ->
/// `%`, `"Left %.0f"` -> `""` (a prefix label, not a unit).
fn unit_from(units: &str) -> String {
    let b = units.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] != b'%' {
            i += 1;
            continue;
        }
        // `%%` is an escaped percent sign, not the start of a conversion.
        if b.get(i + 1) == Some(&b'%') {
            i += 2;
            continue;
        }
        // Consume flags/width/precision up to the conversion character.
        let mut j = i + 1;
        while j < b.len() && !b[j].is_ascii_alphabetic() {
            j += 1;
        }
        return units[(j + 1).min(units.len())..].trim().replace("%%", "%");
    }
    String::new()
}

/// Put the `@`-prefixed parameters after the ordinary ones, keeping the file's
/// order within each group.
///
/// The device keeps two lists, and this is the order both of its own
/// conventions follow:
///
/// * **Stored values.** The captured preset's cab reports
///   `[1, 80, 8000, 0.45, 0, 10]`. In the file's order — `@mic` first, then
///   Distance — that is Distance 80 (range 1–12), Low Cut 8000 (19.9–500) and
///   Level 10 (−60–6): three impossibilities. Ordinary first, `@mic` last, and
///   every value lands: distance 1, 80 Hz, 8000 Hz, 0.45, 0 dB, mic 10.
/// * **Live changes.** A change carries key 29 saying which list its index
///   counts in. Turning Distance reports index 0 of the ordinary list, and
///   turning the mic type reports index 0 of the `@` list (capture 10) — so
///   both are "index 0", and only this split tells them apart.
///
/// The same rule explains the reverb, whose `@trails` is already last in the
/// file, and leaves every model without an `@` parameter untouched.
fn specials_last(defs: &mut [ParamDef]) {
    defs.sort_by_key(|d| d.special);
}

fn param_def(p: &RawParam, controls: &HashMap<String, RawControl>) -> ParamDef {
    let ctl = p
        .display_type
        .as_deref()
        .and_then(|dt| resolve_control(dt, controls));

    let dsp_min = as_f64(&p.min).unwrap_or(0.0);
    let dsp_max = as_f64(&p.max).unwrap_or(1.0);

    let options = ctl.map(labels).unwrap_or_default();
    let kind = match p.value_type {
        2 => ParamKind::Bool,
        // Discrete: a dropdown when the control names its values, otherwise an
        // integer slider (`integer_slider`, `integer_steps`, …).
        0 if !options.is_empty() => ParamKind::Enum,
        _ => ParamKind::Numeric,
    };

    // Display range: an explicit display min/max wins, else scale the DSP range.
    let scale = ctl.and_then(|c| c.dsp_to_display_scale).unwrap_or(1.0);
    let offset = ctl.and_then(|c| c.integer_offset).unwrap_or(0.0);
    let (min, max) = match ctl.and_then(|c| c.minimum_value.zip(c.maximum_value)) {
        Some((lo, hi)) => (lo, hi),
        None => (dsp_min * scale + offset, dsp_max * scale + offset),
    };

    // Units and precision come from the first format rule that has a spec —
    // the base unit. Piecewise rules switch to a larger unit further up the
    // range (`time_ms` reads ms until 1000, then s; `frequency` Hz then kHz),
    // and `ParamDef` carries a single unit, so the base one is the right pick.
    let rules = ctl.map(format_rules).unwrap_or_default();
    let plain_spec = ctl.and_then(|c| c.format.as_ref()).and_then(J::as_str);
    let plain_units = ctl.and_then(|c| c.format_units.as_ref()).and_then(J::as_str);
    let (spec, units) = rules
        .iter()
        .find(|(f, _)| f.as_str().is_some_and(|s| s.contains('%')))
        .map(|(f, u)| (f.as_str().unwrap_or(""), u.as_str().unwrap_or("")))
        .unwrap_or((plain_spec.unwrap_or(""), plain_units.unwrap_or("")));

    let decimals = decimals_from(spec).unwrap_or(if p.value_type == 1 { 1 } else { 0 });
    let unit = unit_from(if units.is_empty() { spec } else { units });

    // A range end that formats to a literal "Off" instead of a number.
    let literal_off = |(f, u): &(&J, &J)| {
        let s = u.as_str().or_else(|| f.as_str()).unwrap_or("");
        s.eq_ignore_ascii_case("off")
    };
    let off_at = if rules.first().is_some_and(literal_off) {
        Some(Edge::Min)
    } else if rules.len() > 1 && rules.last().is_some_and(literal_off) {
        Some(Edge::Max)
    } else {
        None
    };

    ParamDef {
        name: p.name.clone(),
        // `@mic`, `@trails`: the device indexes these separately from the rest.
        special: p.symbolic_id.starts_with('@'),
        kind,
        // Kept separately from `kind` because the two genuinely differ: a
        // slider (`Numeric`) is a float for a percent and an int for a
        // semitone interval, and only this says which.
        wire: match p.value_type {
            0 => WireType::Int,
            2 => WireType::Bool,
            _ => WireType::Float,
        },
        unit,
        min,
        max,
        dsp_min,
        dsp_max,
        decimals,
        off_at,
        options,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every value a real preset's cab reports must land inside the range its
    /// parameter declares. Reading them in the file's order puts three of six
    /// outside — a frequency where a distance belongs — which shows up as
    /// nonsense in the panel rather than as an error.
    #[test]
    fn a_captured_cab_reads_entirely_within_range() {
        let data = include_bytes!("../tests/fixtures/a30-fawn-brt.preset.bin");
        let preset = crate::preset_parser::parse_preset_data(data);
        let cab = preset
            .chain
            .iter()
            .find(|b| b.model_id == Some(53))
            .expect("the captured preset has a cab");
        let model = DB.by_wire_id(53).expect("model 53 resolves");
        assert_eq!(model.category, "Cab");

        assert_eq!(cab.parameters.len(), model.spec.len());
        for (i, value) in cab.parameters.iter().enumerate() {
            let def = model.spec.param(i).unwrap();
            let v = match value {
                crate::preset_parser::ParamValue::Float(f) => *f as f64,
                crate::preset_parser::ParamValue::Int(n) => *n as f64,
                _ => continue,
            };
            assert!(
                v >= def.dsp_min && v <= def.dsp_max,
                "{} = {v} is outside {}..{}", def.name, def.dsp_min, def.dsp_max
            );
        }
        // Ordinary parameters first, `@` ones after — so Distance leads and
        // the mic type, being `@mic`, comes last.
        assert_eq!(model.spec.param(0).unwrap().name, "Distance");
        assert_eq!(model.spec.param(5).unwrap().name, "Mic");
        assert!(model.spec.param(5).unwrap().special);

        // And that is what makes a live change land correctly: turning
        // Distance reports index 0 of the ordinary list, turning the mic type
        // index 0 of the `@` list (capture 10), and both must not collide.
        assert_eq!(model.spec.live_index(0, true), Some(0));  // Distance
        assert_eq!(model.spec.live_index(0, false), Some(5)); // Mic
    }

    /// The reverb's `@trails` is the same case: index 0 of the `@` list, while
    /// index 0 of the ordinary list is Decay. Reading both as "parameter 0"
    /// made turning Trails move the Decay control.
    #[test]
    fn trails_and_decay_are_both_index_zero_of_different_lists() {
        let room = DB.lookup("Room").into_iter().next().expect("the Room reverb");
        assert_eq!(room.spec.param(0).unwrap().name, "Decay");
        let trails = room.spec.live_index(0, false).expect("an @ parameter");
        assert!(room.spec.param(trails).unwrap().special);
        assert_ne!(trails, 0);
        assert_eq!(room.spec.live_index(0, true), Some(0));
    }

    #[test]
    fn database_loads() {
        assert!(DB.models().len() > 500, "got {} models", DB.models().len());
        assert!(
            DB.models().iter().all(|m| !m.name.is_empty()),
            "every model has a display name"
        );
    }

    #[test]
    fn resolves_a_known_model_with_real_ranges() {
        let m = DB.resolve(None, "Gray Flanger").expect("Gray Flanger");
        assert_eq!(m.symbolic_id, "HD2_FlangerGrayFlangerStereo");
        assert_eq!(m.category, "Modulation");
        // 11 params in the file, minus @enabled = the 10 values the device sends.
        assert_eq!(m.spec.len(), 10);

        let rate = m.spec.param(0).unwrap();
        assert_eq!(rate.name, "Rate");
        assert_eq!(rate.kind, ParamKind::Numeric);
        // generic_knob: DSP 0..1 displayed as a "knob to 10".
        assert_eq!((rate.dsp_min, rate.dsp_max), (0.0, 1.0));
        assert_eq!((rate.min, rate.max), (0.0, 10.0));

        // Level is dB — its DSP range *is* its display range, no scaling.
        let level = m.spec.param(6).unwrap();
        assert_eq!(level.name, "Level");
        assert_eq!(level.unit, "dB");
        assert_eq!((level.min, level.max), (-60.0, 6.0));
    }

    #[test]
    fn discrete_labels_are_offset_by_dsp_min() {
        let m = DB.resolve(None, "Gray Flanger").unwrap();
        let sync = m.spec.param(8).unwrap();
        assert_eq!(sync.name, "Note Sync");
        assert_eq!(sync.kind, ParamKind::Enum);
        assert_eq!(sync.options.len(), 19);
        // sync_note is 1-based: the captured wire value 7 is "1/4 Triplet".
        assert_eq!(sync.dsp_min, 1.0);
        let index = |wire: f64| sync.options[(wire - sync.dsp_min) as usize].as_str();
        assert_eq!(index(7.0), "1/4 Triplet");
        assert_eq!(index(8.0), "1/8 Dotted");
        assert_eq!(index(9.0), "1/8");
        assert_eq!(index(6.0), "1/4"); // the default
    }

    #[test]
    fn bool_params_become_checkboxes() {
        let m = DB.resolve(None, "Gray Flanger").unwrap();
        let tempo_sync = m.spec.param(9).unwrap();
        assert_eq!(tempo_sync.name, "Tempo Sync");
        assert_eq!(tempo_sync.kind, ParamKind::Bool);
    }

    #[test]
    fn percent_and_time_scale_to_display_units() {
        let m = DB.resolve(None, "Chamber").expect("Chamber");
        let mix = m.spec.iter().find(|p| p.name == "Mix").expect("Mix");
        assert_eq!((mix.min, mix.max), (0.0, 100.0));
        assert_eq!(mix.unit, "%");

        // time_ms is native seconds on the wire, milliseconds on screen.
        let predelay = m.spec.iter().find(|p| p.name == "Predelay").expect("Predelay");
        assert_eq!(predelay.unit, "ms");
        // Chamber's predelay tops out at 0.2 s on the wire = 200 ms on screen.
        assert_eq!(predelay.dsp_max, 0.2);
        assert!((predelay.max - 200.0).abs() < 1e-6, "predelay max {}", predelay.max);
    }

    #[test]
    fn cut_filters_read_off_at_one_end() {
        let m = DB.resolve(None, "Gray Flanger").unwrap();
        let _ = m;
        // Any model with a low/high cut: the "Off" end is data-driven now.
        let cut = DB
            .models()
            .iter()
            .flat_map(|m| m.spec.iter())
            .find(|p| p.name == "Low Cut" && p.off_at.is_some());
        assert!(cut.is_some(), "a Low Cut param should read Off at its minimum");
        assert_eq!(cut.unwrap().off_at, Some(Edge::Min));
    }

    #[test]
    fn name_lookup_ignores_spacing_and_punctuation() {
        // The device screen and Line 6's files disagree on spacing, casing and
        // punctuation; normalization folds those together so a preset resolves
        // whichever way the name is written.
        for name in ["Color Drive", "63 Spring", "Autofilter"] {
            assert!(DB.resolve(None, name).is_some(), "{name} should resolve");
        }
    }

    /// The id table itself: every wire id observed in the captured A30 Fawn Brt
    /// preset resolves to the model that preset's block actually is.
    ///
    /// This is the whole lookup contract. If `PodGo.sym`'s ordering ever stops
    /// being the id space, these are the assertions that fail.
    #[test]
    fn wire_ids_index_podgo_sym() {
        for (id, symbolic_id, name) in [
            (239u64, "HD2_WahFasselStereo", "Fassel"),
            (224, "HD2_VolPanVolStereo", "Volume"),
            (119, "HD2_FXLoopMono1", "FX Loop 1"),
            (94, "HD2_DistTopSecretODMono", "Top Secret OD"),
            (7, "HD2_AmpA30FawnBrt", "A30 Fawn Brt"),
            (53, "HD2_Cab2x12BlueBell", "2x12 Blue Bell"),
            (100, "HD2_CompressorLAStudioCompMono", "LA Studio Comp"),
            (86, "HD2_DelayTransistorTapeStereo", "Transistor Tape"),
            (210, "HD2_ReverbRoomStereo", "Room"),
            (472, "HD2_EQ_STATIC_ParametricStereo", "Parametric"),
        ] {
            let m = DB.by_wire_id(id).unwrap_or_else(|| panic!("id {id} resolves"));
            assert_eq!(m.symbolic_id, symbolic_id, "id {id}");
            assert_eq!(m.name, name, "id {id}");
        }
        println!("wire ids in the table: {}", DB.entries_by_id().count());
    }

    /// Names the id table disambiguates and name matching cannot: the device
    /// calls these blocks something other than what the files do, or several
    /// models share the name.
    #[test]
    fn ids_resolve_what_names_cannot() {
        // Two "Parametric"s in eq.models; the id picks the right one.
        assert_eq!(DB.lookup("Parametric").len(), 2);
        assert_eq!(DB.by_wire_id(472).unwrap().spec.len(), 12);
        // The device says "Volume Pedal" / "Mono FX Loop"; the files say
        // "Volume" / "FX Loop 1". No name bridge needed.
        assert!(DB.resolve(None, "Volume Pedal").is_none());
        assert_eq!(DB.by_wire_id(224).unwrap().spec.len(), 2);
        assert_eq!(DB.by_wire_id(119).unwrap().spec.len(), 4);
        // Most cab names exist in both cab.models and cabmicirs.models.
        assert_eq!(DB.lookup("2x12 Blue Bell").len(), 2);
        assert_eq!(DB.by_wire_id(53).unwrap().category, "Cab");
        assert_eq!(DB.by_wire_id(53).unwrap().spec.len(), 6);
    }

    /// The A30 Fawn Brt capture, block by block: every model resolves and its
    /// param count matches the number of values the device actually sent.
    #[test]
    fn captured_preset_blocks_match_their_specs() {
        for (name, values) in [
            ("Fassel", 5usize),
            ("Top Secret OD", 2),
            ("LA Studio Comp", 6),
            ("Transistor Tape", 11),
            ("Room", 7),
        ] {
            let m = DB.resolve(None, name).unwrap_or_else(|| panic!("{name} resolves"));
            assert_eq!(m.spec.len(), values, "{name} param count");
        }

        // The preset's EQ block is "Parametric", a name eq.models lists twice,
        // so only its wire id (472) resolves it — see `wire_ids_index_podgo_sym`.
        assert_eq!(DB.resolve(Some(472), "").map(|m| m.spec.len()), Some(12));
        // Room's params, in order, are exactly what the capture decoded:
        // [0.58, 0.029, 150.0, 5000.0, 0.29, 0.0, false].
        let room = DB.resolve(None, "Room").unwrap();
        let names: Vec<&str> = room.spec.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names[1], "Predelay");
        assert_eq!(room.spec.param(1).unwrap().unit, "ms");
        assert_eq!(names[2], "Low Cut");
        assert_eq!(room.spec.param(2).unwrap().unit, "Hz");
        assert_eq!(room.spec.param(5).unwrap().unit, "dB");
    }

    /// Not an assertion about correctness — a visible inventory of models the
    /// data files don't describe, so a firmware that adds models shows up here
    /// instead of silently rendering an empty param list.
    #[test]
    fn report_models_without_params() {
        let empty: Vec<&str> = DB
            .models()
            .iter()
            .filter(|m| m.spec.is_empty())
            .map(|m| m.name.as_str())
            .collect();
        println!("models with no params ({}): {empty:?}", empty.len());
    }
}
