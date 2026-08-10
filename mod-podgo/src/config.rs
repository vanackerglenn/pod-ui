use std::collections::HashMap;
use maplit::*;
use once_cell::sync::Lazy;
use pod_core::model::*;
use pod_core::def;
use pod_mod_pod2::fmt_percent;
use crate::builders::*;
use crate::model::*;

/// Maximum number of parameters a single FX slot can display. Generous upper
/// bound (largest model seen so far is ~10); the dynamic UI only shows as many
/// as the selected model actually has.
pub const MAX_FX_PARAMS: usize = 12;

pub static AMP_MODELS: Lazy<Vec<Amp>> = Lazy::new(|| {
    crate::preset_parser::all_amp_models().into_iter().map(|n| Amp {
        name: n.to_string(),
        ..Default::default()
    }).collect()
});

pub static CAB_MODELS: Lazy<Vec<String>> = Lazy::new(|| {
    crate::preset_parser::all_cab_models().into_iter().map(|n| n.to_string()).collect()
});

pub static REVERB_MODELS: Lazy<Vec<String>> = Lazy::new(|| {
    crate::preset_parser::models_by_category("Reverb").into_iter().map(|n| n.to_string()).collect()
});

pub static DELAY_MODELS: Lazy<Vec<String>> = Lazy::new(|| {
    crate::preset_parser::models_by_category("Delay").into_iter().map(|n| n.to_string()).collect()
});

pub static MOD_MODELS: Lazy<Vec<String>> = Lazy::new(|| {
    crate::preset_parser::models_by_category("Modulation").into_iter().map(|n| n.to_string()).collect()
});

pub static DIST_MODELS: Lazy<Vec<String>> = Lazy::new(|| {
    crate::preset_parser::models_by_category("Distortion").into_iter().map(|n| n.to_string()).collect()
});

pub static WAH_MODELS: Lazy<Vec<String>> = Lazy::new(|| {
    crate::preset_parser::models_by_category("Wah").into_iter().map(|n| n.to_string()).collect()
});

pub static DYN_MODELS: Lazy<Vec<String>> = Lazy::new(|| {
    crate::preset_parser::models_by_category("Dynamic").into_iter().map(|n| n.to_string()).collect()
});

// EQ models for the dedicated Preset EQ block.
pub static EQ_MODELS: Lazy<Vec<String>> = Lazy::new(|| {
    crate::preset_parser::models_by_category("EQ").into_iter().map(|n| n.to_string()).collect()
});

// module parameter labels
pub static STOMP_CONFIG: Lazy<Vec<StompConfig>> = Lazy::new(|| {
    convert_args!(vec!(
        stomp("Kinky Boost").control("Drive").control("Tone").control("Level"),
        stomp("Deranged Master").control("Drive").control("Tone").control("Level"),
        stomp("Minotaur").control("Drive").control("Tone").control("Level"),
        stomp("Teemah!").control("Drive").control("Tone").control("Level"),
        stomp("Heir Apparent").control("Drive").control("Tone").control("Level"),
        stomp("Tone Sovereign").control("Drive").control("Tone").control("Level"),
        stomp("Alpaca Rogue").control("Drive").control("Tone").control("Level"),
        stomp("Compulsive Drive").control("Drive").control("Tone").control("Level"),
        stomp("Dhyana Drive").control("Drive").control("Tone").control("Level"),
        stomp("Horizon Drive").control("Drive").control("Tone").control("Level"),
        stomp("Valve Driver").control("Drive").control("Tone").control("Level"),
        stomp("Top Secret OD").control("Drive").control("Tone").control("Level"),
        stomp("Scream 808").control("Drive").control("Tone").control("Level"),
        stomp("Hedgehog D9").control("Drive").control("Tone").control("Level"),
        stomp("Stupor OD").control("Drive").control("Tone").control("Level"),
        stomp("Deez One Vintage").control("Drive").control("Tone").control("Level"),
        stomp("Deez One Mod").control("Drive").control("Tone").control("Level"),
        stomp("Vermin Dist").control("Drive").control("Tone").control("Level"),
        stomp("KWB").control("Drive").control("Tone").control("Level"),
        stomp("Legendary Drive").control("Drive").control("Tone").control("Level"),
        stomp("Swedish Chainsaw").control("Drive").control("Tone").control("Level"),
        stomp("Arbitrator Fuzz").control("Drive").control("Tone").control("Level"),
        stomp("Pocket Fuzz").control("Drive").control("Tone").control("Level"),
        stomp("Bighorn Fuzz").control("Drive").control("Tone").control("Level"),
        stomp("Triangle Fuzz").control("Drive").control("Tone").control("Level"),
        stomp("Ballistic Fuzz").control("Drive").control("Tone").control("Level"),
        stomp("Industrial Fuzz").control("Drive").control("Tone").control("Level"),
        stomp("Tycoctavia Fuzz").control("Drive").control("Tone").control("Level"),
        stomp("Wringer Fuzz").control("Drive").control("Tone").control("Level"),
        stomp("Thrifter Fuzz").control("Drive").control("Tone").control("Level"),
        stomp("Xenomorph Fuzz").control("Drive").control("Tone").control("Level"),
        stomp("Megaphone").control("Drive").control("Tone").control("Level"),
        stomp("Bitcrusher").control("Drive").control("Tone").control("Level"),
        stomp("Ampeg Scrambler").control("Drive").control("Tone").control("Level"),
        stomp("ZeroAmp Bass DI").control("Drive").control("Tone").control("Level"),
        stomp("Obsidian 7000").control("Drive").control("Tone").control("Level"),
        // Dynamics
        stomp("Deluxe Comp").control("Sustain").control("Level"),
        stomp("Red Squeeze").control("Sustain").control("Level"),
        stomp("Kinky Comp").control("Sustain").control("Level"),
        stomp("Rochester Comp").control("Sustain").control("Level"),
        stomp("LA Studio Comp").control("Sustain").control("Level"),
        stomp("3-Band Comp").control("Sustain").control("Level"),
        stomp("Noise Gate").control("Threshold").control("Decay"),
        stomp("Hard Gate").control("Threshold").control("Decay"),
        stomp("Horizon Gate").control("Threshold").control("Decay"),
        stomp("Autoswell").control("Sensitivity").control("Depth"),
        // EQ
        stomp("Simple EQ").control("Bass").control("Mid").control("Treble"),
        stomp("Low and High Cut").control("Low Cut").control("High Cut"),
        stomp("Low/High Shelf").control("Low Shelf").control("High Shelf"),
        stomp("Parametric").control("Frequency").control("Q").control("Gain"),
        stomp("Tilt").control("Tilt"),
        stomp("10 Band Graphic").skip().skip().skip().skip().skip().skip().skip().skip().skip().skip(),
        stomp("Cali Q Graphic").skip().skip().skip().skip().skip().skip().skip(),
        stomp("Acoustic Sim").control("Body").control("Pickup"),
        // Pitch/Synth
        stomp("Pitch Wham").control("Pitch").control("Mix"),
        stomp("Twin Harmony").control("Harmony").control("Mix"),
        stomp("Simple Pitch").control("Pitch").control("Mix"),
        stomp("Dual Pitch").control("Pitch").control("Mix"),
        stomp("Poly Pitch").control("Pitch").control("Mix"),
        stomp("Poly Wham").control("Pitch").control("Mix"),
        stomp("Poly Capo").control("Pitch").control("Mix"),
        stomp("12 String").control("Mix"),
        stomp("3 Note Generator").skip().skip().skip(),
        stomp("4 OSC Generator").skip().skip().skip(),
        // Filter
        stomp("Mutant Filter").control("Frequency").control("Resonance"),
        stomp("Mystery Filter").control("Frequency").control("Resonance"),
        stomp("Autofilter").control("Frequency").control("Resonance"),
        stomp("Asheville Pattrn").skip().skip().skip().skip(),
        // Wah
        stomp("UK Wah 846").control("Position"),
        stomp("Teardrop 310").control("Position"),
        stomp("Fassel").control("Position"),
        stomp("Weeper").control("Position"),
        stomp("Chrome").control("Position"),
        stomp("Chrome Custom").control("Position"),
        stomp("Throaty").control("Position"),
        stomp("Vetta Wah").control("Position"),
        stomp("Colorful").control("Position"),
        stomp("Conductor").control("Position"),
        // Vol/Pan
        stomp("Volume Pedal").control("Level"),
        stomp("Gain").control("Gain"),
        stomp("Pan").control("Pan"),
        stomp("Stereo Width").control("Width"),
        stomp("Stereo Imager").control("Width").control("Center"),
        // Legacy
        stomp("Tube Drive").control("Drive").control("Tone").control("Level"),
        stomp("Screamer").control("Drive").control("Tone").control("Level"),
        stomp("Overdrive").control("Drive").control("Tone").control("Level"),
        stomp("Classic Dist").control("Drive").control("Tone").control("Level"),
        stomp("Heavy Dist").control("Drive").control("Tone").control("Level"),
        stomp("Buzz Saw").control("Drive").control("Tone").control("Level"),
    ))
});

pub static MOD_CONFIG: Lazy<Vec<ModConfig>> = Lazy::new(|| {
    convert_args!(vec!(
        modc("Optical Trem").control("Speed").control("Depth"),
        modc("60s Bias Trem").control("Speed").control("Depth"),
        modc("Tremolo/Autopan").control("Speed").control("Depth").control("Wave"),
        modc("Harmonic Tremolo").control("Speed").control("Depth"),
        modc("Bleat Chop Trem").control("Speed").control("Depth"),
        modc("Script Mod Phase").control("Speed").control("Depth"),
        modc("Pebble Phaser").control("Speed").control("Depth").control("Feedback"),
        modc("Ubiquitous Vibe").control("Speed").control("Depth"),
        modc("Deluxe Phaser").control("Speed").control("Depth").control("Feedback"),
        modc("Gray Flanger").control("Speed").control("Depth").control("Feedback"),
        modc("Harmonic Flanger").control("Speed").control("Depth").control("Feedback"),
        modc("Courtesan Flange").control("Speed").control("Depth").control("Feedback"),
        modc("Dynamix Flanger").control("Speed").control("Depth").control("Feedback"),
        modc("Chorus").control("Speed").control("Depth").control("Mix"),
        modc("70s Chorus").control("Speed").control("Depth").control("Mix"),
        modc("PlastiChorus").control("Speed").control("Depth").control("Mix"),
        modc("Bubble Vibrato").control("Speed").control("Depth"),
        modc("Trinity Chorus").control("Speed").control("Depth").control("Mix"),
        modc("Vibe Rotary").control("Speed").control("Depth"),
        modc("122 Rotary").control("Speed").control("Depth"),
        modc("145 Rotary").control("Speed").control("Depth"),
        modc("Double Take").control("Mix"),
        modc("Poly Detune").control("Detune").control("Mix"),
        modc("AM Ring Mod").control("Frequency").control("Depth"),
    ))
});

pub static DELAY_CONFIG: Lazy<Vec<DelayConfig>> = Lazy::new(|| {
    convert_args!(vec!(
        delay("Simple Delay").control("Feedback").control("Mix"),
        delay("Mod/Chorus Echo").control("Feedback").control("Mod Speed").control("Depth"),
        delay("Dual Delay").control("Feedback").control("Offset").control("Mix"),
        delay("Multitap 4").control("Feedback").control("Pattern").control("Mix"),
        delay("Multitap 6").control("Feedback").control("Pattern").control("Mix"),
        delay("Ping Pong").control("Feedback").control("Spread").control("Mix"),
        delay("Sweep Echo").control("Feedback").control("Speed").control("Depth"),
        delay("Ducked Delay").control("Feedback").control("Threshold").control("Mix"),
        delay("Reverse Delay").control("Feedback").control("Mix"),
        delay("Vintage Digital").control("Feedback").control("Mix"),
        delay("Vintage Swell").control("Feedback").control("Swim").control("Mix"),
        delay("Pitch Echo").control("Feedback").control("Pitch").control("Mix"),
        delay("Transistor Tape").control("Feedback").control("Flutter").control("Mix"),
        delay("Cosmos Echo").control("Feedback").control("Mix"),
        delay("Harmony Delay").control("Feedback").control("Harmony").control("Mix"),
        delay("Bucket Brigade").control("Feedback").control("Mix"),
        delay("Adriatic Delay").control("Feedback").control("Mix"),
        delay("Adriatic Swell").control("Feedback").control("Swim").control("Mix"),
        delay("Elephant Man").control("Feedback").control("Mix"),
        delay("Multi Pass").skip().skip().skip().skip(),
        delay("Poly Sustain").control("Sustain").control("Mix"),
        delay("Glitch Delay").control("Feedback").control("Glitch").control("Mix"),
    ))
});

/// A model assignable to one of the 4 generic FX slots. POD Go's four
/// freely-assignable blocks can host any effect model; this catalog is the
/// union of all assignable categories, each carrying an (initially generic)
/// ordered param spec.
pub struct FxModel {
    pub name: String,
    pub category: &'static str,
    pub params: ParamSpec,
}

pub static FX_MODELS: Lazy<Vec<FxModel>> = Lazy::new(|| {
    const FX_CATEGORIES: &[&str] = &[
        "Distortion", "Distortion (Legacy)", "Dynamic", "EQ", "Modulation",
        "Delay", "Reverb", "Pitch/Synth", "Filter", "Wah", "Vol/Pan",
    ];
    let mut v = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for cat in FX_CATEGORIES {
        for name in crate::preset_parser::models_by_category(cat) {
            // The model DB has a few same-named entries (e.g. two
            // "4 OSC Generator" type ids); keep names unique so the combo
            // index used by position() lookups stays stable.
            if !seen.insert(name) {
                continue;
            }
            v.push(FxModel {
                name: name.to_string(),
                category: cat,
                params: param_spec_for(name),
            });
        }
    }
    v
});

// Editable per-model parameter data, filled in from the device / owner's
// manual. Editing module_params.toml updates the UI with no code changes —
// the Nth param entry maps to the Nth value the device emits for that model.
#[derive(serde::Deserialize)]
struct TomlParam {
    name: String,
    /// References a built-in or `[types]`-defined param type. Defaults to "percent".
    #[serde(default)] kind: String,
    // Optional inline overrides of the referenced type's fields.
    #[serde(default)] unit: Option<String>,
    #[serde(default)] min: Option<f64>,
    #[serde(default)] max: Option<f64>,
    #[serde(default)] decimals: Option<u8>,
    #[serde(default)] off: Option<String>,   // "min" | "max"
    #[serde(default)] options: Vec<String>,
}
/// A `[types.<name>]` entry: same fields as an inline override, plus an optional
/// widget `kind` ("numeric" | "enum" | "bool"). Inherits the built-in type of
/// the same name (if any), then applies whatever fields are set.
#[derive(serde::Deserialize)]
struct TomlType {
    #[serde(default)] kind: String,
    #[serde(default)] unit: Option<String>,
    #[serde(default)] min: Option<f64>,
    #[serde(default)] max: Option<f64>,
    #[serde(default)] decimals: Option<u8>,
    #[serde(default)] off: Option<String>,
    #[serde(default)] options: Vec<String>,
}
#[derive(serde::Deserialize)]
struct TomlModule { name: String, #[serde(default)] params: Vec<TomlParam> }
#[derive(serde::Deserialize)]
struct TomlModuleFile {
    #[serde(default)] types: HashMap<String, TomlType>,
    #[serde(default)] module: Vec<TomlModule>,
}

fn parse_edge(s: &Option<String>) -> Option<Edge> {
    match s.as_deref().map(|x| x.trim().to_ascii_lowercase()).as_deref() {
        Some("min") => Some(Edge::Min),
        Some("max") => Some(Edge::Max),
        _ => None,
    }
}

fn parse_widget(s: &str) -> ParamKind {
    match s.trim().to_ascii_lowercase().as_str() {
        "enum" => ParamKind::Enum,
        "bool" => ParamKind::Bool,
        _ => ParamKind::Numeric,
    }
}

/// Built-in types overlaid with any `[types]` table entries (each inheriting the
/// built-in of the same name, then applying its set fields).
fn build_types(toml_types: &HashMap<String, TomlType>) -> HashMap<String, ParamType> {
    let mut types = builtin_types();
    for (name, t) in toml_types {
        let mut base = types.get(name).cloned().unwrap_or_default();
        if !t.kind.trim().is_empty() { base.kind = parse_widget(&t.kind); }
        if let Some(u) = &t.unit { base.unit = u.clone(); }
        if let Some(v) = t.min { base.min = v; }
        if let Some(v) = t.max { base.max = v; }
        if let Some(v) = t.decimals { base.decimals = v; }
        if t.off.is_some() { base.off_at = parse_edge(&t.off); }
        if !t.options.is_empty() { base.options = t.options.clone(); }
        types.insert(name.clone(), base);
    }
    types
}

/// Resolve one param against the type registry, then apply its inline overrides.
fn resolve_param(p: TomlParam, types: &HashMap<String, ParamType>) -> ParamDef {
    let key = if p.kind.trim().is_empty() { "percent" } else { p.kind.trim() };
    let base = types.get(key).cloned().unwrap_or_default();
    let mut def = ParamDef::from_type(p.name, &base);
    if let Some(u) = p.unit { def.unit = u; }
    if let Some(v) = p.min { def.min = v; }
    if let Some(v) = p.max { def.max = v; }
    if let Some(v) = p.decimals { def.decimals = v; }
    if p.off.is_some() { def.off_at = parse_edge(&p.off); }
    if !p.options.is_empty() {
        def.options = p.options;
        def.kind = ParamKind::Enum;   // inline options imply a dropdown
    }
    def
}

static MODULE_PARAMS: Lazy<HashMap<String, ParamSpec>> = Lazy::new(|| {
    let src = include_str!("../module_params.toml");
    match toml::from_str::<TomlModuleFile>(src) {
        Ok(f) => {
            let types = build_types(&f.types);
            f.module.into_iter()
                .filter(|m| !m.params.is_empty())
                .map(|m| {
                    let defs = m.params.into_iter()
                        .map(|p| resolve_param(p, &types))
                        .collect();
                    (m.name, ParamSpec::from_defs(defs))
                })
                .collect()
        }
        Err(e) => {
            log::error!("failed to parse module_params.toml: {e}");
            HashMap::new()
        }
    }
});

/// Per-model param spec. `module_params.toml` is the source of truth (fill it in
/// to surface a model's params); the legacy hand-mapped configs are the fallback
/// for anything not yet present there.
fn param_spec_for(name: &str) -> ParamSpec {
    if let Some(s) = MODULE_PARAMS.get(name) {
        return s.clone();
    }
    if let Some(c) = STOMP_CONFIG.iter().find(|c| c.name == name) {
        return spec_from_labels(&c.labels, "stomp");
    }
    if let Some(c) = MOD_CONFIG.iter().find(|c| c.name == name) {
        return spec_from_labels(&c.labels, "mod");
    }
    if let Some(c) = DELAY_CONFIG.iter().find(|c| c.name == name) {
        return spec_from_labels(&c.labels, "delay");
    }
    ParamSpec::default()
}

/// Convert a builder-style labels map (keys `"{prefix}_param{n}"` or
/// `"{prefix}_param{n}_{suffix}"`, `n` starting at 2) into an ordered
/// `ParamSpec` whose index 0 is the first device param.
fn spec_from_labels(labels: &HashMap<String, String>, prefix: &str) -> ParamSpec {
    let key_prefix = format!("{}_param", prefix);
    let mut by_pos: HashMap<usize, String> = HashMap::new();
    let mut max_n = 1usize;
    for (k, v) in labels {
        let Some(rest) = k.strip_prefix(&key_prefix) else { continue; };
        let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
        let Ok(n) = digits.parse::<usize>() else { continue; };
        by_pos.insert(n, v.clone());
        if n > max_n { max_n = n; }
    }
    // The old builders number params from 2, so position = n - 2.
    let labels: Vec<String> = (2..=max_n)
        .map(|n| by_pos.get(&n).cloned().unwrap_or_default())
        .collect();
    ParamSpec::from_strings(labels)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn module_params_load_from_toml() {
        // module_params.toml is the hand-edited source of truth, so assert the
        // loader works rather than specific (editable) param names/counts.
        assert!(!MODULE_PARAMS.is_empty(), "module_params.toml should parse to some specs");
        // A filled model resolves with a non-empty first label.
        let s = param_spec_for("Kinky Boost");
        assert!(!s.is_empty() && s.label(0).is_some());
        // Unknown model -> empty spec.
        assert!(param_spec_for("No Such Model").is_empty());
    }

    #[test]
    fn fx_models_includes_known_effects_with_unique_names() {
        let names: Vec<&str> = FX_MODELS.iter().map(|m| m.name.as_str()).collect();
        assert!(names.contains(&"Deez One Vintage"));
        assert!(names.contains(&"Gray Flanger"));
        assert!(names.contains(&"Chamber"));
        // names must be unique so position() lookups are stable
        let mut sorted = names.clone(); sorted.sort(); sorted.dedup();
        assert_eq!(sorted.len(), names.len());
    }
}

pub static CONFIG: Lazy<Config> = Lazy::new(|| {
    let mut controls: HashMap<String, Control> = convert_args!(hashmap!(
        // switches
        "noise_gate_enable" => SwitchControl { cc: 22, addr: 32 + 22, ..def() },
        "wah_enable" => SwitchControl { cc: 43, addr: 32 + 43, ..def() },
        "amp_enable" => SwitchControl { cc: 111, addr: 32 + 111, inverted: true },
        "compressor_enable" => SwitchControl { cc: 26, addr: 32 + 26, ..def() },
        "tuner_enable" => MidiSwitchControl { cc: 69 },
        // preamp
        "amp_select" => Select { cc: 11, addr: 32 + 12 , ..def() },
        "drive" => RangeControl { cc: 13, addr: 32 + 13, format: fmt_percent!(), ..def() },
        "bass" => RangeControl { cc: 14, addr: 32 + 14, format: fmt_percent!(), ..def() },
        "mid" => RangeControl { cc: 15, addr: 32 + 15, format: fmt_percent!(), ..def() },
        "treble" => RangeControl { cc: 16, addr: 32 + 16, format: fmt_percent!(), ..def() },
        "presence" => RangeControl { cc: 21, addr: 32 + 21, format: fmt_percent!(), ..def() },
        "chan_volume" => RangeControl { cc: 17, addr: 32 + 17, format: fmt_percent!(), ..def() },
        // cab
        "cab_select" => Select { cc: 71, addr: 32 + 71, ..def() },
        // noise gate
        "gate_threshold" => RangeControl { cc: 23, addr: 32 + 23,
            config: RangeConfig::Function {
                from_midi: |v| (96u16.saturating_sub(v as u16)).min(96),
                to_midi: |v| (96u8.saturating_sub(v as u8)).min(96),
            },
            format: Format::Data(FormatData { k: 1.0, b: -96.0, format: "{val} db".into() }), ..def() },
        "gate_decay" => RangeControl { cc: 24, addr: 32 + 24, format: fmt_percent!(), ..def() },
        // compressor
        "compressor_threshold" => RangeControl { cc: 9, addr: 32 + 9,
            format: Format::Data(FormatData { k: 63.0/127.0, b: -63.0, format: "{val:1.1f} db".into() }),
            ..def() },
        "compressor_gain" => RangeControl { cc: 5, addr: 32 + 5,
            format: Format::Data(FormatData { k: 16.0/127.0, b: 0.0, format: "{val:1.1f} db".into() }),
            ..def() },
        // volume pedal
        "vol_level" => RangeControl { cc: 7, addr: 32 + 7, format: fmt_percent!(), ..def() },
        // wah
        "wah_select" => Select { cc: 91, addr: 32 + 91, ..def() },
        "wah_level" => RangeControl { cc: 4, addr: 32 + 4, format: fmt_percent!(), ..def() },

        // --- 4 freely-assignable FX slots ---------------------------------
        // Select/enable carry inert cc keys (read-only viewer; cc <= 127, away
        // from the device's real CCs 1/2/49-69). The per-slot param controls are
        // added below as VirtualRangeControls (no cc/addr → MIDI-silent, and no
        // cc-space limit), since the param widgets are built dynamically.
        "fx1_select" => Select { cc: 72, addr: 32 + 72, ..def() },
        "fx1_enable" => SwitchControl { cc: 73, addr: 32 + 73, ..def() },
        "fx2_select" => Select { cc: 82, addr: 32 + 82, ..def() },
        "fx2_enable" => SwitchControl { cc: 83, addr: 32 + 83, ..def() },
        "fx3_select" => Select { cc: 93, addr: 32 + 93, ..def() },
        "fx3_enable" => SwitchControl { cc: 94, addr: 32 + 94, ..def() },
        "fx4_select" => Select { cc: 103, addr: 32 + 103, ..def() },
        "fx4_enable" => SwitchControl { cc: 104, addr: 32 + 104, ..def() },

        // --- fixed blocks not previously modelled -------------------------
        "volume_enable" => SwitchControl { cc: 114, addr: 32 + 114, ..def() },
        "volume_position" => SwitchControl { cc: 115, addr: 32 + 115, ..def() },
        // Preset EQ is the dedicated EQ block; it carries a model selector (EQ
        // type). Its params are native dB/Hz units (not 0..1), so they're not
        // surfaced on percent sliders yet — selector + enable only for now.
        "preset_eq_select" => Select { cc: 116, addr: 32 + 116, ..def() },
        "preset_eq_enable" => SwitchControl { cc: 117, addr: 32 + 117, ..def() },
        "fx_loop_enable" => SwitchControl { cc: 118, addr: 32 + 118, ..def() },
        "fx_loop_mix" => RangeControl { cc: 119, addr: 32 + 119, format: fmt_percent!(), ..def() },

        // name change button
        "name_change" => Button {},
    ));

    // Per-slot FX param value holders. Virtual (no cc/addr) so they emit no
    // MIDI and aren't cc-space-limited; the param widgets that display them are
    // built dynamically per selected model (up to MAX_FX_PARAMS each).
    for n in 1..=4 {
        for k in 1..=MAX_FX_PARAMS {
            controls.insert(
                format!("fx{n}_param{k}"),
                VirtualRangeControl { format: fmt_percent!(), ..def() }.into(),
            );
        }
    }

    Config {
        name: "POD Go".to_string(),
        family: 0x0021,
        member: 0x0007,

        // Large enough to cover every control's (inert) addr; POD Go never
        // uses the standard buffer-dump path, so this is just a safe upper
        // bound for the per-control addr writes on UI edits. Max addr = 151.
        program_size: 256,
        program_num: 128,
        program_name_addr: 0,
        program_name_length: 16,

        pc_manual_mode: None,
        pc_tuner: None,
        pc_offset: None,

        amp_models: AMP_MODELS.clone(),
        cab_models: CAB_MODELS.clone(),
        effects: REVERB_MODELS.iter().map(|n| Effect { name: n.clone(), ..Default::default() }).collect(),
        toggles: vec![],

        controls,
        init_controls: convert_args!(vec!(
            "amp_select",
            "cab_select",
            "fx1_select",
            "fx2_select",
            "fx3_select",
            "fx4_select",
            "wah_select",
            "noise_gate_enable",
            "tuner_enable",
        )),

        out_cc_edit_buffer_dump_req: vec![],
        in_cc_edit_buffer_dump_req: vec![],

        flags: DeviceFlags::empty(),
        midi_quirks: MidiQuirks::empty(),
    }
});
