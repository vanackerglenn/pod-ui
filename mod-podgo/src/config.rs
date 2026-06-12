use std::collections::HashMap;
use maplit::*;
use once_cell::sync::Lazy;
use pod_core::model::*;
use pod_core::def;
use pod_mod_pod2::{short, long, steps, fmt_percent};
use crate::builders::*;
use crate::model::*;

pub static AMP_MODELS: Lazy<Vec<Amp>> = Lazy::new(|| {
    pod_usb::all_amp_models().into_iter().map(|n| Amp {
        name: n.to_string(),
        ..Default::default()
    }).collect()
});

pub static CAB_MODELS: Lazy<Vec<String>> = Lazy::new(|| {
    pod_usb::all_cab_models().into_iter().map(|n| n.to_string()).collect()
});

pub static REVERB_MODELS: Lazy<Vec<String>> = Lazy::new(|| {
    pod_usb::models_by_category("Reverb").into_iter().map(|n| n.to_string()).collect()
});

pub static DELAY_MODELS: Lazy<Vec<String>> = Lazy::new(|| {
    pod_usb::models_by_category("Delay").into_iter().map(|n| n.to_string()).collect()
});

pub static MOD_MODELS: Lazy<Vec<String>> = Lazy::new(|| {
    pod_usb::models_by_category("Modulation").into_iter().map(|n| n.to_string()).collect()
});

pub static DIST_MODELS: Lazy<Vec<String>> = Lazy::new(|| {
    pod_usb::models_by_category("Distortion").into_iter().map(|n| n.to_string()).collect()
});

pub static WAH_MODELS: Lazy<Vec<String>> = Lazy::new(|| {
    pod_usb::models_by_category("Wah").into_iter().map(|n| n.to_string()).collect()
});

pub static DYN_MODELS: Lazy<Vec<String>> = Lazy::new(|| {
    pod_usb::models_by_category("Dynamic").into_iter().map(|n| n.to_string()).collect()
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

pub static CONFIG: Lazy<Config> = Lazy::new(|| {
    let controls: HashMap<String, Control> = convert_args!(hashmap!(
        // switches
        "noise_gate_enable" => SwitchControl { cc: 22, addr: 32 + 22, ..def() },
        "wah_enable" => SwitchControl { cc: 43, addr: 32 + 43, ..def() },
        "stomp_enable" => SwitchControl { cc: 25, addr: 32 + 25, ..def() },
        "mod_enable" => SwitchControl { cc: 50, addr: 32 + 50, ..def() },
        "delay_enable" => SwitchControl { cc: 28, addr: 32 + 28, ..def() },
        "reverb_enable" => SwitchControl { cc: 36, addr: 32 + 36, ..def() },
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
        // reverb
        "reverb_select" => Select { cc: 37, addr: 32 + 37, ..def() },
        "reverb_decay" => RangeControl { cc: 38, addr: 32 + 38, format: fmt_percent!(), ..def() },
        "reverb_tone" => RangeControl { cc: 39, addr: 32 + 39, format: fmt_percent!(), ..def() },
        "reverb_pre_delay" => RangeControl { cc: 40, addr: 32 + 40, format: fmt_percent!(), ..def() },
        "reverb_level" => RangeControl { cc: 18, addr: 32 + 18, format: fmt_percent!(), ..def() },
        // stomp
        "stomp_select" => Select { cc: 75, addr: 32 + 75, ..def() },
        "stomp_param2" => RangeControl { cc: 79, addr: 32 + 79, format: fmt_percent!(), ..def() },
        "stomp_param3" => RangeControl { cc: 80, addr: 32 + 80, format: fmt_percent!(), ..def() },
        "stomp_param4" => RangeControl { cc: 81, addr: 32 + 81, format: fmt_percent!(), ..def() },
        "stomp_param5" => RangeControl { cc: 82, addr: 32 + 82, format: fmt_percent!(), ..def() },
        "stomp_param6" => RangeControl { cc: 83, addr: 32 + 83, format: fmt_percent!(), ..def() },
        // mod
        "mod_select" => Select { cc: 58, addr: 32 + 58, ..def() },
        "mod_speed" => VirtualRangeControl {
            config: long!(0, 16383),
            format: Format::Data(FormatData { k: 14.9/16383.0, b: 0.1, format: "{val:1.2f} Hz".into() }),
            ..def() },
        "mod_speed:msb" => RangeControl { cc: 29, addr: 32 + 29, ..def() },
        "mod_speed:lsb" => RangeControl { cc: 61, addr: 32 + 61, ..def() },
        "mod_param2" => RangeControl { cc: 52, addr: 32 + 52, format: fmt_percent!(), ..def() },
        "mod_param3" => RangeControl { cc: 53, addr: 32 + 53, format: fmt_percent!(), ..def() },
        "mod_param4" => RangeControl { cc: 54, addr: 32 + 54, format: fmt_percent!(), ..def() },
        "mod_mix" => RangeControl { cc: 56, addr: 32 + 56, format: fmt_percent!(), ..def() },
        // delay
        "delay_select" => Select { cc: 88, addr: 32 + 88, ..def() },
        "delay_time" => VirtualRangeControl {
            config: long!(0, 16383),
            format: Format::Data(FormatData { k: 1980.0/16383.0, b: 20.0, format: "{val:1.0f} ms".into() }),
            ..def() },
        "delay_time:msb" => RangeControl { cc: 30, addr: 32 + 30, ..def() },
        "delay_time:lsb" => RangeControl { cc: 62, addr: 32 + 62, ..def() },
        "delay_param2" => RangeControl { cc: 33, addr: 32 + 33, format: fmt_percent!(), ..def() },
        "delay_param3" => RangeControl { cc: 35, addr: 32 + 35, format: fmt_percent!(), ..def() },
        "delay_param4" => RangeControl { cc: 85, addr: 32 + 85, format: fmt_percent!(), ..def() },
        "delay_mix" => RangeControl { cc: 34, addr: 32 + 34, format: fmt_percent!(), ..def() },
        // volume pedal
        "vol_level" => RangeControl { cc: 7, addr: 32 + 7, format: fmt_percent!(), ..def() },
        // wah
        "wah_select" => Select { cc: 91, addr: 32 + 91, ..def() },
        "wah_level" => RangeControl { cc: 4, addr: 32 + 4, format: fmt_percent!(), ..def() },
        // name change button
        "name_change" => Button {},
    ));

    Config {
        name: "POD Go".to_string(),
        family: 0x0021,
        member: 0x0007,

        program_size: 72 * 2 + 16,
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
            "reverb_select",
            "stomp_select",
            "mod_select",
            "delay_select",
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
