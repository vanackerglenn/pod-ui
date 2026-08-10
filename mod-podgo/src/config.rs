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
        let (id, _, name) = crate::preset_parser::module_db_entries()
            .find(|(_, _, name)| *name == "2x12 Blue Bell")
            .expect("2x12 Blue Bell is a known cab");
        let spec = param_spec_for_id(Some(id), "");
        assert_eq!(spec.len(), 6, "{name} should resolve from its id alone");
        // The name alone can't do this: it is shared with the mic'd-IR cab.
        assert!(crate::models_db::DB.resolve(None, name).is_none());
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
