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

/// The name shown for a block position the preset doesn't fill.
pub const EMPTY_MODEL: &str = "(empty)";

/// The models a given block can host, each carrying its numeric wire id so a
/// preset block resolves by id rather than by name.
///
/// Built from [`crate::models_db`], whose id table is `PodGo.sym`. The fixed
/// blocks use the same `FxModel` shape as the four assignable FX slots so they
/// can share the dynamic param-widget machinery in `module.rs`.
fn block_models(categories: &[&'static str]) -> Vec<FxModel> {
    // Index 0 is always "no model here". Without it an unfilled block falls
    // back to whatever sorts first in its category — which is why empty slots
    // used to display "Alpaca Rouge".
    let mut v: Vec<FxModel> = vec![FxModel {
        name: EMPTY_MODEL.to_string(),
        category: "",
        ids: vec![],
        params: ParamSpec::default(),
    }];
    let mut by_name: HashMap<String, usize> = HashMap::new();
    let all = categories.is_empty();
    for (id, m) in crate::models_db::DB.entries_by_id() {
        if !all && !categories.contains(&m.category) {
            continue;
        }
        // One row per display name, but keep every id that leads to it so no
        // preset block can fail to match.
        match by_name.get(&m.name) {
            Some(&i) => v[i].ids.push(id),
            None => {
                by_name.insert(m.name.clone(), v.len());
                v.push(FxModel {
                    name: m.name.clone(),
                    category: m.category,
                    ids: vec![id],
                    params: m.spec.clone(),
                });
            }
        }
    }
    v[1..].sort_by(|a, b| a.name.cmp(&b.name));
    v
}

/// The amp block. POD Go's amp slot can host a full amp or a preamp.
pub static AMP_BLOCK_MODELS: Lazy<Vec<FxModel>> = Lazy::new(|| block_models(&["Amp", "Preamp"]));
/// The cab block: legacy cabs and the mic'd-IR cabs, which have different
/// param lists (the IR cabs put Distance third, legacy cabs second).
pub static CAB_BLOCK_MODELS: Lazy<Vec<FxModel>> = Lazy::new(|| block_models(&["Cab", "Cab/IR"]));
pub static WAH_BLOCK_MODELS: Lazy<Vec<FxModel>> = Lazy::new(|| block_models(&["Wah"]));
pub static EQ_BLOCK_MODELS: Lazy<Vec<FxModel>> = Lazy::new(|| block_models(&["EQ"]));
pub static VOLUME_BLOCK_MODELS: Lazy<Vec<FxModel>> = Lazy::new(|| block_models(&["Vol/Pan"]));
pub static FX_LOOP_BLOCK_MODELS: Lazy<Vec<FxModel>> = Lazy::new(|| block_models(&["Send/Return"]));

// Name-only views, kept because pod_core's Config wants them. Derived from the
// lists above so a selector index means the same thing in both.
pub static AMP_MODELS: Lazy<Vec<Amp>> = Lazy::new(|| {
    AMP_BLOCK_MODELS.iter().skip(1).map(|m| Amp { name: m.name.clone(), ..Default::default() }).collect()
});

pub static CAB_MODELS: Lazy<Vec<String>> = Lazy::new(|| {
    CAB_BLOCK_MODELS.iter().skip(1).map(|m| m.name.clone()).collect()
});

pub static WAH_MODELS: Lazy<Vec<String>> = Lazy::new(|| {
    WAH_BLOCK_MODELS.iter().skip(1).map(|m| m.name.clone()).collect()
});

pub static EQ_MODELS: Lazy<Vec<String>> = Lazy::new(|| {
    EQ_BLOCK_MODELS.iter().skip(1).map(|m| m.name.clone()).collect()
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

pub static DYN_MODELS: Lazy<Vec<String>> = Lazy::new(|| {
    crate::preset_parser::models_by_category("Dynamic").into_iter().map(|n| n.to_string()).collect()
});

/// How many positions POD Go's signal chain has. Slots are numbered from 1;
/// the chain array's index 0 and trailing entry are the flow's input and
/// output, not blocks.
pub const CHAIN_SLOTS: usize = 10;

/// The controller name prefix for a chain position.
///
/// The UI is keyed by *position*, not by what kind of block sits there. A
/// preset gives ten positions each holding a model id, and that id is enough to
/// name the model and lay out its parameters — so loading a patch needs no
/// notion of category at all. (It used to map each block's category onto one of
/// ten per-category prefixes, which meant any category that mapping didn't
/// anticipate — Looper, the Send/Return variants — had nowhere to go, displayed
/// as empty, and consumed a slot that belonged to something else.)
///
/// Category still matters for *changing* a block: pick a type, then a model.
/// Each entry in [`ALL_MODELS`] carries its own, so it comes from the model
/// rather than from the position.
pub fn slot_prefix(slot: usize) -> String {
    format!("slot{slot}")
}

/// Every model the device knows, indexed so a wire id resolves in one step.
/// Index 0 is the explicit "(empty)" entry.
pub static ALL_MODELS: Lazy<Vec<FxModel>> = Lazy::new(|| {
    let mut v = block_models(&[]);
    v[1..].sort_by(|a, b| (a.category, &a.name).cmp(&(b.category, &b.name)));
    v
});

/// Resolve a wire model id to its index in [`ALL_MODELS`].
pub fn model_index_for_id(id: u64) -> Option<usize> {
    ALL_MODELS.iter().position(|m| m.ids.contains(&id))
}

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
    /// Every numeric wire id that resolves to this entry. Usually one, but the
    /// data files list a few models twice under the same name with identical
    /// params (both "Parametric"s in `eq.models`), and each copy has its own
    /// id. The UI shows one row; a preset block matches on any of its ids.
    pub ids: Vec<u64>,
    pub params: ParamSpec,
}

pub static FX_MODELS: Lazy<Vec<FxModel>> = Lazy::new(|| {
    // POD Go's looper occupies one of the assignable effect blocks, so it
    // belongs here — without it a looper resolves to nothing, displays as empty
    // and still eats an FX slot.
    const FX_CATEGORIES: &[&str] = &[
        "Distortion", "Distortion (Legacy)", "Dynamic", "EQ", "Modulation",
        "Delay", "Reverb", "Pitch/Synth", "Filter", "Wah", "Vol/Pan", "Looper",
    ];
    // Built from the wire ids: each entry carries the id a preset block will be
    // matched on, and resolves its params by that id rather than by name.
    let mut v = block_models(FX_CATEGORIES);
    v[1..].sort_by_key(|m| {
        (
            FX_CATEGORIES.iter().position(|c| *c == m.category).unwrap_or(usize::MAX),
            m.name.clone(),
        )
    });
    v
});

/// Per-model param spec, from POD Go Edit's own model database (see
/// [`crate::models_db`]). The legacy hand-mapped builder configs remain the
/// fallback for anything the data files don't describe.
///
/// `id` is the numeric model id the preset block carries — the unambiguous
/// key. Pass `None` only when it isn't available.
fn param_spec_for_id(id: Option<u64>, name: &str) -> ParamSpec {
    if let Some(m) = crate::models_db::DB.resolve(id, name) {
        if !m.spec.is_empty() {
            return m.spec.clone();
        }
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

fn param_spec_for(name: &str) -> ParamSpec {
    param_spec_for_id(None, name)
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
    fn param_specs_come_from_the_model_database() {
        // A known model resolves with real params out of mod-podgo/data/.
        let s = param_spec_for("Kinky Boost");
        assert_eq!(s.len(), 3);
        assert_eq!(s.label(0), Some("Drive"));
        // Unknown model -> empty spec.
        assert!(param_spec_for("No Such Model").is_empty());
    }

    #[test]
    fn wire_id_resolves_a_model_with_no_name_of_its_own() {
        // A Cab block in a loaded preset carries only its numeric id — the name
        // "2x12 Blue Bell" appears nowhere in the payload. Resolving by id has
        // to work without a usable name.
        let spec = param_spec_for_id(Some(53), "");
        assert_eq!(spec.len(), 6, "2x12 Blue Bell should resolve from its id");
        // The name alone can't do this: it is shared with the mic'd-IR cab.
        assert!(crate::models_db::DB.resolve(None, "2x12 Blue Bell").is_none());
    }

    #[test]
    fn ambiguous_names_are_refused_rather_than_guessed() {
        // "Sweep Echo" is both the HD2 and the DL4 model. Without an id there
        // is nothing to pick on, and picking wrong would misalign every
        // positional param — so the database declines to answer. (config's
        // own legacy builder tables may still supply a fallback spec.)
        assert_eq!(crate::models_db::DB.lookup("Sweep Echo").len(), 2);
        assert!(crate::models_db::DB.resolve(None, "Sweep Echo").is_none());
    }

    /// End-to-end over a real preset: parse the captured A30 Fawn Brt patch and
    /// check every block resolves to a spec that matches the number of values
    /// the device actually sent for it.
    ///
    /// A spec shorter or longer than the value array means the params are
    /// misaligned — every slider would show its neighbour's value. That is the
    /// failure this guards, and it is invisible to the per-model unit tests
    /// because those never go through a parsed preset.
    #[test]
    fn captured_preset_resolves_every_block_to_a_matching_spec() {
        let data = include_bytes!("../tests/fixtures/a30-fawn-brt.preset.bin");
        let preset = crate::preset_parser::parse_preset_data(data);
        assert_eq!(preset.modules.len(), 10, "the patch has 10 blocks");

        let mut unresolved = Vec::new();
        for m in &preset.modules {
            let spec = param_spec_for_id(m.model_id, &m.name);
            if spec.is_empty() {
                unresolved.push(format!("{} ({})", m.name, m.category));
                continue;
            }
            assert_eq!(
                spec.len(),
                m.parameters.len(),
                "{}: spec has {} params but the device sent {} values",
                m.name,
                spec.len(),
                m.parameters.len()
            );
        }
        assert!(
            unresolved.is_empty(),
            "blocks with no param spec: {unresolved:?}"
        );
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

        // The Volume and FX Loop blocks have no visible model selector (POD Go
        // fixes each to a single model), but they still need a select value so
        // the same param-rebuild wiring applies. Virtual: no cc/addr, no MIDI.
        "volume_select" => VirtualSelect {},
        "fx_loop_select" => VirtualSelect {},

        // name change button
        "name_change" => Button {},
    ));

    // Per-block param value holders. Virtual (no cc/addr) so they emit no MIDI
    // and aren't cc-space-limited; the widgets that display them are built
    // dynamically per selected model (up to MAX_FX_PARAMS each).
    // One set of controls per chain position. Virtual (no cc/addr) so they emit
    // no MIDI and aren't cc-space-limited; the widgets that display them are
    // built dynamically from the model that position holds.
    for slot in 1..=CHAIN_SLOTS {
        let prefix = slot_prefix(slot);
        controls.insert(format!("{prefix}_select"), VirtualSelect {}.into());
        controls.insert(format!("{prefix}_enable"), VirtualSelect {}.into());
        for k in 1..=MAX_FX_PARAMS {
            controls.insert(
                format!("{prefix}_param{k}"),
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


#[cfg(test)]
mod ui_dump {
    use super::*;

    /// Every value in the captured preset maps onto a control value — nothing
    /// is silently dropped for being in native units.
    ///
    /// This is what the old 0..1-only rule got wrong: Fassel's 455 Hz, Room's
    /// 5000 Hz and Parametric's 0.707 Q were all skipped, leaving those sliders
    /// at zero while the block looked correctly identified.
    #[test]
    fn every_captured_value_maps_to_a_control_value() {
        let data = include_bytes!("../tests/fixtures/a30-fawn-brt.preset.bin");
        let preset = crate::preset_parser::parse_preset_data(data);
        let mut checked = 0;
        for m in &preset.modules {
            let spec = param_spec_for_id(m.model_id, &m.name);
            for (i, pv) in m.parameters.iter().enumerate() {
                let Some(def) = spec.param(i) else { continue };
                assert!(
                    crate::handler::control_value(def, pv).is_some(),
                    "{}: param {} ({}) = {:?} did not map",
                    m.name, i, def.name, pv
                );
                checked += 1;
            }
        }
        assert!(checked >= 60, "only {checked} values checked");
    }

    /// Spot-check the mapping against values whose display units are known, so
    /// a scaling regression shows up as a wrong number rather than silence.
    #[test]
    fn native_unit_values_land_where_expected() {
        let data = include_bytes!("../tests/fixtures/a30-fawn-brt.preset.bin");
        let preset = crate::preset_parser::parse_preset_data(data);
        let room = preset.modules.iter().find(|m| m.name == "Room").expect("Room block");
        let spec = param_spec_for_id(room.model_id, &room.name);

        // Room: [Decay 0.58, Predelay 0.029 s, LowCut 150 Hz, HighCut 5000 Hz,
        //        Mix 0.29, Level 0.0 dB, trails false]
        let display = |i: usize| {
            let def = spec.param(i).unwrap();
            let v = crate::handler::control_value(def, &room.parameters[i]).unwrap();
            def.min + (v as f64 / 127.0) * (def.max - def.min)
        };
        assert!((display(2) - 150.0).abs() < 60.0, "low cut {} Hz", display(2));
        assert!((display(3) - 5000.0).abs() < 400.0, "high cut {} Hz", display(3));
        assert!((display(4) - 29.0).abs() < 1.0, "mix {}%", display(4));
        assert!((display(5) - 0.0).abs() < 1.0, "level {} dB", display(5));
        // The trails flag is a checkbox, not a scale.
        assert_eq!(spec.param(6).unwrap().kind, crate::model::ParamKind::Bool);
    }

    /// Every block of a real preset resolves to a model whose param count
    /// matches the values the device sent for that position.
    ///
    /// Identifying the model isn't enough — a spec of the wrong length
    /// misaligns every slider on that block.
    #[test]
    fn every_chain_position_resolves_to_a_matching_spec() {
        let data = include_bytes!("../tests/fixtures/a30-fawn-brt.preset.bin");
        let preset = crate::preset_parser::parse_preset_data(data);
        for slot in 1..=CHAIN_SLOTS {
            let Some(block) = preset.chain.get(slot) else { continue };
            let Some(id) = block.model_id else { continue };
            let index = model_index_for_id(id)
                .unwrap_or_else(|| panic!("position {slot}: model id {id} does not resolve"));
            assert_eq!(
                ALL_MODELS[index].params.len(),
                block.parameters.len(),
                "position {slot} ({}): spec {} vs {} values",
                ALL_MODELS[index].name, ALL_MODELS[index].params.len(), block.parameters.len()
            );
        }
    }

    /// An unused block reads as empty, not as whatever model happens to sort
    /// first in its category.
    ///
    /// A preset with fewer than four effects leaves FX slots unfilled;
    /// `reset_managed_blocks` sets their selector to 0, so index 0 of every
    /// block list has to mean "nothing here". It used to be a real model, which
    /// is why empty slots displayed "Alpaca Rouge".
    #[test]
    fn an_unfilled_block_reads_as_empty() {
        assert_eq!(ALL_MODELS[0].name, EMPTY_MODEL, "index 0 must be the empty entry");
        assert!(ALL_MODELS[0].params.is_empty(), "the empty entry has no params");
        assert!(ALL_MODELS[0].ids.is_empty(), "no wire id maps to the empty entry");
    }

    /// Every model the device can report resolves, whatever its category.
    ///
    /// Loading is keyed by position and model id, so a category no block used
    /// to host — Looper, the Send/Return variants — is no longer a special
    /// case. This asserts the catalogue is genuinely complete rather than
    /// filtered down to the categories the old per-category lists covered.
    #[test]
    fn every_known_model_resolves_by_id() {
        let mut unresolved = Vec::new();
        for (id, m) in crate::models_db::DB.entries_by_id() {
            if model_index_for_id(id).is_none() {
                unresolved.push((id, m.name.clone(), m.category));
            }
        }
        assert!(unresolved.is_empty(), "models with no entry: {unresolved:?}");
        // Loopers in particular used to have nowhere to go.
        let loopers: Vec<&str> = ALL_MODELS
            .iter()
            .filter(|m| m.category == "Looper")
            .map(|m| m.name.as_str())
            .collect();
        assert!(!loopers.is_empty(), "loopers should be selectable");
    }

    /// Each chain position drives its own controls, so what a preset holds at
    /// position N lands on `slotN`.
    #[test]
    fn every_chain_position_syncs_to_its_own_controls() {
        use std::sync::{Arc, Mutex};
        use pod_core::controller::Controller;
        use pod_core::store::Store;

        let controller = Arc::new(Mutex::new(Controller::new(CONFIG.controls.clone())));
        let data = include_bytes!("../tests/fixtures/a30-fawn-brt.preset.bin");
        let preset = crate::preset_parser::parse_preset_data(data);
        crate::handler::sync_controller_from_preset(&controller, &preset);

        let ctrl = controller.lock().unwrap();
        for (slot, expected) in [
            (1usize, "Fassel"), (2, "Volume"), (3, "FX Loop 1"), (4, "Top Secret OD"),
            (5, "A30 Fawn Brt"), (6, "2x12 Blue Bell"), (7, "LA Studio Comp"),
            (8, "Transistor Tape"), (9, "Room"), (10, "Parametric"),
        ] {
            let idx = ctrl.get(&format!("{}_select", slot_prefix(slot))).unwrap() as usize;
            assert_eq!(ALL_MODELS[idx].name, expected, "position {slot}");
        }
    }

    /// A position the model database can't resolve shows empty *in place*, and
    /// every other position keeps its own.
    ///
    /// This is the failure that kept recurring while the UI was keyed by
    /// category: an unplaceable block (a Looper, a Send/Return variant) took a
    /// slot that wasn't its own and shifted everything after it one position
    /// left. Keyed by position, an unresolvable model can only blank itself.
    #[test]
    fn an_unresolvable_block_only_blanks_its_own_position() {
        use std::sync::{Arc, Mutex};
        use pod_core::controller::Controller;
        use pod_core::store::Store;

        let controller = Arc::new(Mutex::new(Controller::new(CONFIG.controls.clone())));
        let data = include_bytes!("../tests/fixtures/a30-fawn-brt.preset.bin");
        let mut preset = crate::preset_parser::parse_preset_data(data);
        // A model id nothing knows, at position 3.
        preset.chain[3].model_id = Some(u64::MAX);
        preset.chain[3].name = None;
        crate::handler::sync_controller_from_preset(&controller, &preset);

        let ctrl = controller.lock().unwrap();
        assert_eq!(ctrl.get("slot3_select"), Some(0), "the unknown block reads empty");
        for (slot, expected) in [(4usize, "Top Secret OD"), (5, "A30 Fawn Brt"), (10, "Parametric")] {
            let idx = ctrl.get(&format!("{}_select", slot_prefix(slot))).unwrap() as usize;
            assert_eq!(ALL_MODELS[idx].name, expected, "position {slot} must not shift");
        }
    }

    /// Drive the real preset->controller sync with the captured patch and read
    /// the control values back out. This is the whole pipeline short of the
    /// widgets, so if these values are right, any remaining problem is in the
    /// UI layer alone.
    #[test]
    #[ignore]
    fn dump_chain_occupancy() {
        let data = include_bytes!("../tests/fixtures/a30-fawn-brt.preset.bin");
        let preset = crate::preset_parser::parse_preset_data(data);
        for (slot, id) in preset.chain.iter().enumerate() {
            let who = preset.modules.iter().find(|m| m.slot as usize == slot);
            println!("chain[{slot:>2}] id={:?} name={:?}", id.model_id, id.name);
        }
    }

    #[test]
    #[ignore]
    fn dump_controller_after_sync() {
        use std::sync::{Arc, Mutex};
        use pod_core::controller::Controller;
        use pod_core::store::Store;

        let data = include_bytes!("../tests/fixtures/a30-fawn-brt.preset.bin");
        let preset = crate::preset_parser::parse_preset_data(data);
        let controller = Arc::new(Mutex::new(Controller::new(CONFIG.controls.clone())));
        crate::handler::sync_controller_from_preset(&controller, &preset);

        let ctrl = controller.lock().unwrap();
        for slot in 1..=CHAIN_SLOTS {
            let prefix = slot_prefix(slot);
            let sel = ctrl.get(&format!("{prefix}_select"));
            let name = sel.and_then(|i| ALL_MODELS.get(i as usize)).map(|m| m.name.as_str());
            let vals: Vec<String> = (1..=MAX_FX_PARAMS)
                .filter_map(|k| ctrl.get(&format!("{prefix}_param{k}")))
                .map(|v| v.to_string())
                .collect();
            println!("{prefix:<10} select={sel:?} model={name:?} params=[{}]", vals.join(", "));
        }
    }
}

