use std::collections::HashMap;

/// The display kind of a single parameter, used to pick the right widget and
/// formatting. Sourced from `module_params.toml` (`kind = "..."`).
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub enum ParamKind {
    #[default]
    Percent,
    Db,
    Hz,
    Ms,
    Time,
    Semitones,
    Enum,
    Bool,
    Int,
    Unknown,
}

impl ParamKind {
    pub fn parse(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "percent" => ParamKind::Percent,
            "db" => ParamKind::Db,
            "hz" => ParamKind::Hz,
            "ms" => ParamKind::Ms,
            "time" => ParamKind::Time,
            "semitones" => ParamKind::Semitones,
            "enum" => ParamKind::Enum,
            "bool" => ParamKind::Bool,
            "int" => ParamKind::Int,
            _ => ParamKind::Unknown,
        }
    }
}

/// A single parameter definition: a display name, its kind, and (for enums) the
/// option labels. An empty `name` marks a position that exists on the device
/// but isn't surfaced in the UI yet.
#[derive(Clone, Debug, Default)]
pub struct ParamDef {
    pub name: String,
    pub kind: ParamKind,
    pub options: Vec<String>,
}

/// Ordered parameter spec for a single FX model. The device exposes params
/// positionally (no names over USB), so names/kinds/order are hand-mapped per
/// model (in `module_params.toml`). The Nth entry maps to the Nth device value.
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