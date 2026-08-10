use std::collections::HashMap;

/// How a parameter is rendered. The device sends only positional values
/// (a normalized f32 0..1 for continuous params, or an int index for discrete
/// ones), so this — and the range metadata on [`ParamDef`] — are hand-authored
/// display hints, not anything the device reports.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ParamKind {
    /// A continuous value on a slider (percent, dB, Hz, ms, cents, semitones…).
    #[default]
    Numeric,
    /// A discrete choice shown as a dropdown (needs `options`).
    Enum,
    /// An on/off switch shown as a checkbox.
    Bool,
}

/// Which end of a numeric range reads "Off" instead of a number (e.g. a Low Cut
/// filter shows "Off" at its minimum, a High Cut at its maximum).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Edge { Min, Max }

/// A reusable parameter type: the widget plus its display unit/range.
///
/// Only the legacy builder tables in `config.rs` still use these; per-model
/// params now come from Line 6's own data via [`crate::models_db`], which
/// builds [`ParamDef`]s directly.
#[derive(Clone, Debug)]
pub struct ParamType {
    pub kind: ParamKind,
    pub unit: String,
    pub min: f64,
    pub max: f64,
    pub decimals: u8,
    pub off_at: Option<Edge>,
    pub options: Vec<String>,
}

impl Default for ParamType {
    fn default() -> Self {
        ParamType { kind: ParamKind::Numeric, unit: String::new(),
            min: 0.0, max: 100.0, decimals: 0, off_at: None, options: Vec::new() }
    }
}

impl ParamType {
    fn numeric(unit: &str, min: f64, max: f64, decimals: u8) -> Self {
        ParamType { kind: ParamKind::Numeric, unit: unit.to_string(),
            min, max, decimals, off_at: None, options: Vec::new() }
    }
}

/// Built-in reusable param types. Ranges tagged `estimate` are first-pass
/// guesses; they only apply to models still served by the legacy builder
/// fallback, since [`crate::models_db`] supplies real per-param ranges.
pub fn builtin_types() -> HashMap<String, ParamType> {
    let mut m = HashMap::new();
    let mut t = |name: &str, ty: ParamType| { m.insert(name.to_string(), ty); };
    t("percent",   ParamType::numeric("%", 0.0, 100.0, 0));
    t("percent1",  ParamType::numeric("", 0.0, 10.0, 1));        // "knob to 10" scale, e.g. 8.8
    t("mix",       ParamType::numeric("%", 0.0, 100.0, 0));
    t("db",        ParamType::numeric("dB", -60.0, 12.0, 1));    // estimate
    t("level",     ParamType::numeric("dB", -60.0, 12.0, 1));    // estimate
    t("hz",        ParamType::numeric("Hz", 20.0, 20000.0, 0));  // estimate
    t("freq",      ParamType::numeric("Hz", 20.0, 20000.0, 0));  // estimate
    t("ms",        ParamType::numeric("ms", 0.0, 1000.0, 0));    // estimate
    t("semitones", ParamType::numeric("st", -24.0, 24.0, 0));    // estimate
    t("cents",     ParamType::numeric("¢", -50.0, 50.0, 1));
    t("pan",       ParamType::numeric("", -100.0, 100.0, 0));    // Left/Center/Right labels: TODO
    t("int",       ParamType::numeric("", 0.0, 10.0, 0));        // estimate
    // Cut filters: one end reads "Off". Ranges are estimates.
    t("lowcut",    ParamType { off_at: Some(Edge::Min), ..ParamType::numeric("Hz", 20.0, 500.0, 0) });
    t("highcut",   ParamType { off_at: Some(Edge::Max), ..ParamType::numeric("Hz", 500.0, 20000.0, 0) });
    t("enum",      ParamType { kind: ParamKind::Enum, ..Default::default() });
    t("bool",      ParamType { kind: ParamKind::Bool, ..Default::default() });
    t("unknown",   ParamType::default());
    m
}

/// A single parameter definition: a display name plus its resolved type
/// (widget + unit/range). An empty `name` marks a position that exists on the
/// device but isn't surfaced in the UI yet.
///
/// There are **two ranges** here and they are not interchangeable. `dsp_min`
/// and `dsp_max` bound the value that travels on the USB wire; `min` and `max`
/// bound what the user sees. For a percent param those are `0..1` and `0..100`;
/// for a dB param they are the same numbers. Read `dsp_*` when building a write
/// and `min`/`max` when driving a widget — see `models_db` for the mapping.
#[derive(Clone, Debug)]
pub struct ParamDef {
    pub name: String,
    pub kind: ParamKind,
    pub unit: String,
    /// Display range, in `unit`s.
    pub min: f64,
    pub max: f64,
    /// Wire range, in DSP units. For an [`ParamKind::Enum`], the wire value is
    /// an index offset by `dsp_min`: `options[value - dsp_min]`.
    pub dsp_min: f64,
    pub dsp_max: f64,
    pub decimals: u8,
    pub off_at: Option<Edge>,
    pub options: Vec<String>,
}

impl Default for ParamDef {
    fn default() -> Self {
        ParamDef::from_type(String::new(), &ParamType::default())
    }
}

impl ParamDef {
    /// Build a param from a resolved [`ParamType`] and a display name.
    pub fn from_type(name: String, t: &ParamType) -> Self {
        ParamDef { name, kind: t.kind, unit: t.unit.clone(), min: t.min, max: t.max,
            dsp_min: t.min, dsp_max: t.max,
            decimals: t.decimals, off_at: t.off_at, options: t.options.clone() }
    }
}

/// Ordered parameter spec for a single FX model. The device exposes params
/// positionally (no names over USB), so the Nth entry maps to the Nth value the
/// device sends. Built from Line 6's model files by [`crate::models_db`].
#[derive(Clone, Debug, Default)]
pub struct ParamSpec {
    params: Vec<ParamDef>,
}

impl ParamSpec {
    /// Build from bare names (kind defaults to Percent). Used by the legacy
    /// config fallback and tests.
    pub fn new(labels: &[&str]) -> Self {
        ParamSpec {
            params: labels.iter().map(|s| ParamDef { name: s.to_string(), ..Default::default() }).collect(),
        }
    }
    pub fn from_strings(labels: Vec<String>) -> Self {
        ParamSpec {
            params: labels.into_iter().map(|name| ParamDef { name, ..Default::default() }).collect(),
        }
    }
    pub fn from_defs(params: Vec<ParamDef>) -> Self {
        ParamSpec { params }
    }
    /// The display name at `idx`, or None for an out-of-range or unsurfaced
    /// (empty-name) position.
    pub fn label(&self, idx: usize) -> Option<&str> {
        self.params.get(idx).map(|p| p.name.as_str()).filter(|s| !s.is_empty())
    }
    pub fn param(&self, idx: usize) -> Option<&ParamDef> {
        self.params.get(idx)
    }
    pub fn iter(&self) -> std::slice::Iter<'_, ParamDef> {
        self.params.iter()
    }
    pub fn len(&self) -> usize { self.params.len() }
    pub fn is_empty(&self) -> bool { self.params.is_empty() }
}

#[derive(Clone, Debug)]
pub struct StompConfig {
    pub name: String,
    pub labels: HashMap<String, String>,
}

#[derive(Clone, Debug)]
pub struct ModConfig {
    pub name: String,
    pub labels: HashMap<String, String>,
}

#[derive(Clone, Debug)]
pub struct DelayConfig {
    pub name: String,
    pub labels: HashMap<String, String>,
}

pub trait ConfigAccess {
    fn name(&self) -> &String;
    fn labels(&self) -> &HashMap<String, String>;
}

impl ConfigAccess for ModConfig {
    fn name(&self) -> &String {
        &self.name
    }
    fn labels(&self) -> &HashMap<String, String> {
        &self.labels
    }
}

impl ConfigAccess for StompConfig {
    fn name(&self) -> &String {
        &self.name
    }
    fn labels(&self) -> &HashMap<String, String> {
        &self.labels
    }
}

impl ConfigAccess for DelayConfig {
    fn name(&self) -> &String {
        &self.name
    }
    fn labels(&self) -> &HashMap<String, String> {
        &self.labels
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn param_spec_indexes_named_params() {
        let s = ParamSpec::new(&["Drive", "Tone", "Level"]);
        assert_eq!(s.label(0), Some("Drive"));
        assert_eq!(s.label(2), Some("Level"));
        assert_eq!(s.label(3), None);
        assert_eq!(s.len(), 3);
    }
}