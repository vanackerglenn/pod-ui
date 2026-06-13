// Binary preset parser — ports helix_usb Python format analysis to Rust
// Parses slot sections, module entries, parameters, and footswitch data

use std::collections::HashMap;
use once_cell::sync::Lazy;

// === Module Database (hex ID → (category, name)) ===
// Ported from https://github.com/kempline/helix_usb/blob/main/modules.py

static MODULE_DB: Lazy<HashMap<&'static str, (&'static str, &'static str)>> = Lazy::new(|| {
    let mut m = HashMap::new();
    macro_rules! def { ($id:expr, $cat:expr, $name:expr) => { m.insert($id, ($cat, $name)); }; }
    // Distortion
    def!("cd0184", "Distortion", "Kinky Boost");
    def!("cd01fe", "Distortion", "Deranged Master");
    def!("64", "Distortion", "Minotaur");
    def!("cd012e", "Distortion", "Teemah!");
    def!("cd0223", "Distortion", "Heir Apparent");
    def!("cd0225", "Distortion", "Tone Sovereign");
    def!("cd0229", "Distortion", "Alpaca Rogue");
    def!("60", "Distortion", "Compulsive Drive");
    def!("cd020d", "Distortion", "Dhyana Drive");
    def!("cd0246", "Distortion", "Horizon Drive");
    def!("69", "Distortion", "Valve Driver");
    def!("66", "Distortion", "Top Secret OD");
    def!("65", "Distortion", "Scream 808");
    def!("61", "Distortion", "Hedgehog D9");
    def!("cd0154", "Distortion", "Stupor OD");
    def!("cd01fc", "Distortion", "Deez One Vintage");
    def!("cd01fa", "Distortion", "Deez One Mod");
    def!("6a", "Distortion", "Vermin Dist");
    def!("cd0120", "Distortion", "KWB");
    def!("cd0234", "Distortion", "Legendary Drive");
    def!("cd0248", "Distortion", "Swedish Chainsaw");
    def!("5f", "Distortion", "Arbitrator Fuzz");
    def!("cd0253", "Distortion", "Pocket Fuzz");
    def!("cd0236", "Distortion", "Bighorn Fuzz");
    def!("67", "Distortion", "Triangle Fuzz");
    def!("cd0251", "Distortion", "Ballistic Fuzz");
    def!("62", "Distortion", "Industrial Fuzz");
    def!("68", "Distortion", "Tycoctavia Fuzz");
    def!("cd0140", "Distortion", "Wringer Fuzz");
    def!("cd0182", "Distortion", "Thrifter Fuzz");
    def!("cd022b", "Distortion", "Xenomorph Fuzz");
    def!("63", "Distortion", "Megaphone");
    def!("cd0122", "Distortion", "Bitcrusher");
    def!("cd0209", "Distortion", "Ampeg Scrambler");
    def!("cd020b", "Distortion", "ZeroAmp Bass DI");
    def!("cd015f", "Distortion", "Obsidian 7000");
    // Dynamic
    def!("77", "Dynamic", "Deluxe Comp");
    def!("79", "Dynamic", "Red Squeeze");
    def!("cd01dc", "Dynamic", "Kinky Comp");
    def!("cd022d", "Dynamic", "Rochester Comp");
    def!("78", "Dynamic", "LA Studio Comp");
    def!("cd016f", "Dynamic", "3-Band Comp");
    def!("7b", "Dynamic", "Noise Gate");
    def!("7a", "Dynamic", "Hard Gate");
    def!("cd0255", "Dynamic", "Horizon Gate");
    def!("cd0169", "Dynamic", "Autoswell");
    // EQ
    def!("cc84", "EQ", "Simple EQ");
    def!("cc82", "EQ", "Low and High Cut");
    def!("cd020f", "EQ", "Low/High Shelf");
    def!("cc83", "EQ", "Parametric");
    def!("cd0211", "EQ", "Tilt");
    def!("cc81", "EQ", "10 Band Graphic");
    def!("cd0143", "EQ", "Cali Q Graphic");
    def!("cd0244", "EQ", "Acoustic Sim");
    // Modulation
    def!("cca3", "Modulation", "Optical Trem");
    def!("cca2", "Modulation", "60s Bias Trem");
    def!("cd0124", "Modulation", "Tremolo/Autopan");
    def!("cd013e", "Modulation", "Harmonic Tremolo");
    def!("cd0186", "Modulation", "Bleat Chop Trem");
    def!("cc9f", "Modulation", "Script Mod Phase");
    def!("cd022f", "Modulation", "Pebble Phaser");
    def!("cca0", "Modulation", "Ubiquitous Vibe");
    def!("cd0126", "Modulation", "Deluxe Phaser");
    def!("cc9d", "Modulation", "Gray Flanger");
    def!("cc9e", "Modulation", "Harmonic Flanger");
    def!("cc9c", "Modulation", "Courtesan Flange");
    def!("cd0130", "Modulation", "Dynamix Flanger");
    def!("cc9b", "Modulation", "Chorus");
    def!("cc9a", "Modulation", "70s Chorus");
    def!("cd0177", "Modulation", "PlastiChorus");
    def!("cca4", "Modulation", "Bubble Vibrato");
    def!("cca7", "Modulation", "Trinity Chorus");
    def!("ccb1", "Modulation", "Vibe Rotary");
    def!("ccaf", "Modulation", "122 Rotary");
    def!("ccb0", "Modulation", "145 Rotary");
    def!("cd0188", "Modulation", "Double Take");
    def!("cd0240", "Modulation", "Poly Detune");
    def!("cca1", "Modulation", "AM Ring Mod");
    // Delay
    def!("50", "Delay", "Simple Delay");
    def!("4f", "Delay", "Mod/Chorus Echo");
    def!("cd011f", "Delay", "Dual Delay");
    def!("59", "Delay", "Multitap 4");
    def!("5a", "Delay", "Multitap 6");
    def!("5b", "Delay", "Ping Pong");
    def!("51", "Delay", "Sweep Echo");
    def!("4d", "Delay", "Ducked Delay");
    def!("cd011d", "Delay", "Reverse Delay");
    def!("cd014b", "Delay", "Vintage Digital");
    def!("cd015b", "Delay", "Vintage Swell");
    def!("cd014d", "Delay", "Pitch Echo");
    def!("52", "Delay", "Transistor Tape");
    def!("cd018a", "Delay", "Cosmos Echo");
    def!("57", "Delay", "Harmony Delay");
    def!("4c", "Delay", "Bucket Brigade");
    def!("4b", "Delay", "Adriatic Delay");
    def!("cd0159", "Delay", "Adriatic Swell");
    def!("4e", "Delay", "Elephant Man");
    def!("cd01e8", "Delay", "Multi Pass");
    def!("cd0243", "Delay", "Poly Sustain");
    def!("cd0238", "Delay", "Glitch Delay");
    // Reverb
    def!("cd01ea", "Reverb", "Glitz");
    def!("cd01f0", "Reverb", "Ganymede");
    def!("cd01f3", "Reverb", "Searchlights");
    def!("cd01f1", "Reverb", "Plateaux");
    def!("cd01ee", "Reverb", "Double Tank");
    def!("ccf6", "Reverb", "Plate");
    def!("ccf7", "Reverb", "Room");
    def!("ccf0", "Reverb", "Chamber");
    def!("ccf3", "Reverb", "Hall");
    def!("ccf2", "Reverb", "Echo");
    def!("ccf9", "Reverb", "Tile");
    def!("ccef", "Reverb", "Cave");
    def!("ccf1", "Reverb", "Ducking");
    def!("ccf4", "Reverb", "Octo");
    def!("ccee", "Reverb", "63 Spring");
    def!("ccf8", "Reverb", "Spring");
    def!("ccf5", "Reverb", "Particle Verb");
    // Pitch/Synth
    def!("ccb6", "Pitch/Synth", "Pitch Wham");
    def!("ccb7", "Pitch/Synth", "Twin Harmony");
    def!("cd0128", "Pitch/Synth", "Simple Pitch");
    def!("cd012a", "Pitch/Synth", "Dual Pitch");
    def!("cd023d", "Pitch/Synth", "Poly Pitch");
    def!("cd023f", "Pitch/Synth", "Poly Wham");
    def!("cd023e", "Pitch/Synth", "Poly Capo");
    def!("cd0242", "Pitch/Synth", "12 String");
    def!("cd0179", "Pitch/Synth", "3 Note Generator");
    def!("cd017b", "Pitch/Synth", "4 OSC Generator");
    def!("cd0175", "Pitch/Synth", "4 OSC Generator");
    // Filter
    def!("cc89", "Filter", "Mutant Filter");
    def!("cc8a", "Filter", "Mystery Filter");
    def!("cd012c", "Filter", "Autofilter");
    def!("cd0213", "Filter", "Asheville Pattrn");
    // Wah
    def!("cd0110", "Wah", "UK Wah 846");
    def!("cd010e", "Wah", "Teardrop 310");
    def!("cd010d", "Wah", "Fassel");
    def!("cd0112", "Wah", "Weeper");
    def!("cd010a", "Wah", "Chrome");
    def!("cd0109", "Wah", "Chrome Custom");
    def!("cd010f", "Wah", "Throaty");
    def!("cd0111", "Wah", "Vetta Wah");
    def!("cd010b", "Wah", "Colorful");
    def!("cd010c", "Wah", "Conductor");
    // Vol/Pan
    def!("cd0105", "Vol/Pan", "Volume Pedal");
    def!("cd0104", "Vol/Pan", "Gain");
    def!("cd0107", "Vol/Pan", "Pan");
    def!("cd018c", "Vol/Pan", "Stereo Width");
    def!("cd024c", "Vol/Pan", "Stereo Imager");
    // Amp
    def!("2c", "Amp", "WhoWatt 100");
    def!("23", "Amp", "Soup Pro");
    def!("24", "Amp", "Stone Age 185");
    def!("cd018d", "Amp", "Voltage Queen");
    def!("26", "Amp", "Tweed Blues Nrm");
    def!("25", "Amp", "Tweed Blues Brt");
    def!("cd021d", "Amp", "Fullerton Nrm");
    def!("cd021b", "Amp", "Fullerton Brt");
    def!("cd021c", "Amp", "Fullerton Jump");
    def!("cd0217", "Amp", "GrammaticoLG Nrm");
    def!("cd0215", "Amp", "GrammaticoLG Brt");
    def!("cd0216", "Amp", "GrammaticoLG Jmp");
    def!("2b", "Amp", "US Small Tweed");
    def!("cd024f", "Amp", "US Princess");
    def!("27", "Amp", "US Deluxe Nrm");
    def!("28", "Amp", "US Deluxe Vib");
    def!("29", "Amp", "US Double Nrm");
    def!("2a", "Amp", "US Double Vib");
    def!("1d", "Amp", "Mail Order Twin");
    def!("13", "Amp", "Divided Duo");
    def!("18", "Amp", "Interstate Zed");
    def!("cd0180", "Amp", "Derailed Ingrid");
    def!("19", "Amp", "Jazz Rivet 120");
    def!("14", "Amp", "Essex A15");
    def!("15", "Amp", "Essex A30");
    def!("08", "Amp", "A30 Fawn Nrm");
    def!("07", "Amp", "A30 Fawn Brt");
    def!("cd0132", "Amp", "Matchstick Ch1");
    def!("cd0133", "Amp", "Matchstick Ch2");
    def!("cd0134", "Amp", "Matchstick Jump");
    def!("1e", "Amp", "Mandarin 80");
    def!("0c", "Amp", "Brit J45 Nrm");
    def!("0b", "Amp", "Brit J45 Brt");
    def!("cd01de", "Amp", "Brit Trem Nrm");
    def!("cd01df", "Amp", "Brit Trem Brt");
    def!("cd01e0", "Amp", "Brit Trem Jump");
    def!("11", "Amp", "Brit Plexi Nrm");
    def!("0f", "Amp", "Brit Plexi Brt");
    def!("10", "Amp", "Brit Plexi Jump");
    def!("0e", "Amp", "Brit P75 Nrm");
    def!("0d", "Amp", "Brit P75 Brt");
    def!("0a", "Amp", "Brit 2204");
    def!("cd0200", "Amp", "Placater Clean");
    def!("cd01f4", "Amp", "Placater Dirty");
    def!("cd01e4", "Amp", "Cartographer");
    def!("16", "Amp", "German Mahadeva");
    def!("17", "Amp", "German Ubersonic");
    def!("cd0202", "Amp", "Cali Texas Ch1");
    def!("cd01f6", "Amp", "Cali Texas Ch2");
    def!("cd0139", "Amp", "Cali IV Rhythm 1");
    def!("cd013a", "Amp", "Cali IV Rhythm 2");
    def!("cd0138", "Amp", "Cali IV Lead");
    def!("12", "Amp", "Cali Rectifire");
    def!("cd014f", "Amp", "Archetype Clean");
    def!("cd0150", "Amp", "Archetype Lead");
    def!("09", "Amp", "ANGL Meteor");
    def!("20", "Amp", "Solo Lead Clean");
    def!("21", "Amp", "Solo Lead Crunch");
    def!("22", "Amp", "Solo Lead OD");
    def!("1f", "Amp", "PV Panama");
    def!("cd0231", "Amp", "Revv Gen Purple");
    def!("cd0221", "Amp", "Revv Gen Red");
    def!("cd0259", "Amp", "Das Benzin Mega");
    def!("cd0257", "Amp", "Das Benzin Lead");
    def!("1b", "Amp", "Line 6 Elektrik");
    def!("1a", "Amp", "Line 6 Doom");
    def!("1c", "Amp", "Line 6 Epic");
    def!("cd0142", "Amp", "Line 6 2204 Mod");
    def!("cd0148", "Amp", "Line 6 Fatality");
    def!("cd0153", "Amp", "Line 6 Litigator");
    def!("cd015d", "Amp", "Line 6 Badonk");
    // Cab
    def!("33", "Cab", "Soup Pro Ellipse");
    def!("34", "Cab", "1x8 Small Tweed");
    def!("cd024d", "Cab", "1x10 US Princess");
    def!("2f", "Cab", "1x12 Field Coil");
    def!("cd0227", "Cab", "1x12 Fullerton");
    def!("cd0228", "Cab", "1x12 Grammatico");
    def!("31", "Cab", "1x12 US Deluxe");
    def!("cd024e", "Cab", "1x12 US Princess");
    def!("2e", "Cab", "1x12 Celest 12H");
    def!("2d", "Cab", "1x12 Blue Bell");
    def!("30", "Cab", "1x12 Lead 80");
    def!("cd0164", "Cab", "1x12 Cali IV");
    def!("cd0163", "Cab", "1x12 Cali Ext");
    def!("36", "Cab", "2x12 Double C12N");
    def!("39", "Cab", "2x12 Mail C12Q");
    def!("37", "Cab", "2x12 Interstate");
    def!("38", "Cab", "2x12 Jazz Rivet");
    def!("3a", "Cab", "2x12 Silver Bell");
    def!("35", "Cab", "2x12 Blue Bell");
    def!("cd0167", "Cab", "2x12 Match H30");
    def!("cd0166", "Cab", "2x12 Match G25");
    def!("3d", "Cab", "4x10 Tweed P10R");
    def!("47", "Cab", "4x12 WhoWatt 100");
    def!("43", "Cab", "4x12 Mandarin EM");
    def!("42", "Cab", "4x12 Greenback25");
    def!("41", "Cab", "4x12 Greenback20");
    def!("3f", "Cab", "4x12 Blackback30");
    def!("3e", "Cab", "4x12 1960 T75");
    def!("46", "Cab", "4x12 Uber V30");
    def!("45", "Cab", "4x12 Uber T75");
    def!("40", "Cab", "4x12 Cali V30");
    def!("48", "Cab", "4x12 XXL V30");
    def!("44", "Cab", "4x12 SoloLead EM");
    // Send/Return
    def!("ccfa", "Send/Return", "Send/Return");
    def!("ccfb", "Send/Return", "Send L");
    def!("cce8", "Send/Return", "Send R");
    def!("cce9", "Send/Return", "Return L");
    def!("cc8d", "Send/Return", "Return R");
    def!("cc8e", "Send/Return", "FX Loop L");
    def!("ccfe", "Send/Return", "FX Loop R");
    def!("ccec", "Send/Return", "Send L/R");
    def!("cc91", "Send/Return", "Return L/R");
    // IR
    def!("cc95", "Impulse Response", "1024 samples");
    def!("cc96", "Impulse Response", "2048 samples");
    // Legacy (not exhaustive — add as needed)
    def!("cd01b2", "Distortion (Legacy)", "Tube Drive");
    def!("cd01af", "Distortion (Legacy)", "Screamer");
    def!("cd01ad", "Distortion (Legacy)", "Overdrive");
    def!("cd01a3", "Distortion (Legacy)", "Classic Dist");
    def!("cd01a7", "Distortion (Legacy)", "Heavy Dist");
    def!("cd01a2", "Distortion (Legacy)", "Buzz Saw");
    def!("cce7", "Amp", "Studio Tube Pre");
    m
});

pub fn lookup_module_type(type_id: &str) -> Option<(&'static str, &'static str)> {
    MODULE_DB.get(type_id).copied()
}

// === Parameter Values ===

#[derive(Debug, Clone)]
pub enum ParamValue {
    Bool(bool),
    Float(f32),
    Int(u8),
    Raw(Vec<u8>),
}

impl std::fmt::Display for ParamValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ParamValue::Bool(b) => write!(f, "{}", if *b { "ON" } else { "OFF" }),
            ParamValue::Float(v) => write!(f, "{:.1}", v),
            ParamValue::Int(v) => write!(f, "{}", v),
            ParamValue::Raw(v) => write!(f, "raw[{}]", v.len()),
        }
    }
}

// === Module Info ===

#[derive(Debug, Clone)]
pub struct ModuleInfo {
    pub name: String,
    pub category: String,
    pub slot: u8,
    pub bypassed: bool,
    pub type_id: String,
    pub parameters: Vec<ParamValue>,
}

// === Full Preset Data ===

#[derive(Debug, Clone, Default)]
pub struct PresetData {
    pub modules: Vec<ModuleInfo>,
    pub footswitches: Vec<FootSwitchInfo>,
}

#[derive(Debug, Clone)]
pub struct FootSwitchInfo {
    pub label: String,
    pub custom_label: String,
    pub led_color: i32,
}

// === Name -> category reverse lookup ===
//
// The preset binary identifies each model with per-firmware numeric IDs that do
// NOT match MODULE_DB's id scheme. The human-readable model name, however, is
// stored directly in the preset and is unique, so we resolve a module's
// category from its name.
static NAME_TO_CATEGORY: Lazy<HashMap<&'static str, &'static str>> = Lazy::new(|| {
    let mut m = HashMap::new();
    for (_id, (cat, name)) in MODULE_DB.iter() {
        m.insert(*name, *cat);
    }
    m
});

pub fn category_for_name(name: &str) -> Option<&'static str> {
    NAME_TO_CATEGORY.get(name).copied()
}

// === Binary Parser (MessagePack) ===
//
// A Pod Go preset is a single MessagePack document wrapped in a short binary
// preamble. The outer map carries the real payload under key 104, which is
// itself a MessagePack *stream* of three values: the "l6-helix" marker string,
// a binary header, and the preset map. Within the preset map, each signal-chain
// module is a sub-map carrying its display name under key 5, a node id under
// key 6, a bypass flag under key 7 and its slot index under key 8.

use rmpv::Value;

const KEY_NAME: u64 = 5;
const KEY_ID: u64 = 6;
// Key 7 is the block's ENABLED flag (true = active). Note: hx-protocol.md
// mislabeled this as "bypass"; the device data shows true = on.
const KEY_ENABLED: u64 = 7;
const KEY_SLOT: u64 = 8;

pub fn parse_preset_data(data: &[u8]) -> PresetData {
    let mut preset = PresetData::default();
    let Some(root) = parse_preset_value(data) else {
        return preset;
    };
    collect_modules(&root, &mut preset.modules);

    // Attach parameter values. The signal chain lives at root[0][22] and is
    // indexed by slot, so chain[slot] holds the block's parameter array.
    let chain = as_map(&root)
        .and_then(|m| map_get(m, 0))
        .and_then(as_map)
        .and_then(|m| map_get(m, 22))
        .and_then(|v| v.as_array());
    if let Some(chain) = chain {
        for m in &mut preset.modules {
            if let Some(block) = chain.get(m.slot as usize) {
                m.parameters = extract_params(block);
            }
        }

        // Amp & Cab blocks carry no named entry in root[3][8]; their identity
        // lives only in the chain meta (block[20][24][25] = model id). Recover
        // them here so the fixed Amp/Cab selectors populate. The chain index is
        // the slot. Limited to Amp/Cab — the categories whose small model ids
        // resolve reliably via the DB.
        let known_slots: std::collections::HashSet<u8> =
            preset.modules.iter().map(|m| m.slot).collect();
        for (slot, block) in chain.iter().enumerate() {
            if known_slots.contains(&(slot as u8)) {
                continue;
            }
            let Some(id) = chain_block_model_id(block) else { continue; };
            let hex = format_type_id(id);
            if let Some((cat, name)) = lookup_module_type(&hex) {
                if matches!(cat, "Amp" | "Cab") {
                    preset.modules.push(ModuleInfo {
                        name: name.to_string(),
                        category: cat.to_string(),
                        slot: slot as u8,
                        bypassed: false,
                        type_id: hex,
                        parameters: extract_params(block),
                    });
                }
            }
        }
    }

    preset.modules.sort_by_key(|m| m.slot);
    preset.modules.dedup_by(|a, b| a.name == b.name && a.slot == b.slot);
    preset
}

/// Locate and decode the inner preset map from the raw transfer bytes.
fn parse_preset_value(data: &[u8]) -> Option<Value> {
    // The payload stream begins with the "l6-helix" marker string. Back up one
    // byte to include its MessagePack fixstr header (0xa9) and decode from there.
    let marker = b"l6-helix";
    let idx = data.windows(marker.len()).position(|w| w == marker)?;
    let start = idx.saturating_sub(1);
    let mut cur: &[u8] = &data[start..];
    // Stream layout: "l6-helix", <binary header>, <preset map>. Read a handful
    // of values and return the first Map encountered (the preset itself).
    for _ in 0..4 {
        match rmpv::decode::read_value(&mut cur) {
            Ok(v @ Value::Map(_)) => return Some(v),
            Ok(_) => continue,
            Err(_) => break,
        }
    }
    None
}

fn map_get<'a>(map: &'a [(Value, Value)], key: u64) -> Option<&'a Value> {
    map.iter().find(|(k, _)| k.as_u64() == Some(key)).map(|(_, v)| v)
}

fn as_map(v: &Value) -> Option<&Vec<(Value, Value)>> {
    match v {
        Value::Map(m) => Some(m),
        _ => None,
    }
}

/// Extract a chain block's parameter values. The block stores its values under
/// key 20, which contains several sub-maps keyed by a count; the populated one
/// (vs. an empty snapshot copy) holds the value array at key 4.
fn extract_params(block: &Value) -> Vec<ParamValue> {
    let Some(sub) = as_map(block).and_then(|m| map_get(m, 20)).and_then(as_map) else {
        return vec![];
    };
    let mut best: Option<&Vec<Value>> = None;
    for (k, v) in sub {
        if k.as_u64() == Some(24) {
            continue; // skip the block meta (enabled/model/position)
        }
        if let Some(arr) = as_map(v).and_then(|m| map_get(m, 4)).and_then(|x| x.as_array()) {
            if best.map_or(true, |b| arr.len() > b.len()) {
                best = Some(arr);
            }
        }
    }
    best.map(|arr| arr.iter().map(value_to_param).collect()).unwrap_or_default()
}

/// A chain block's model id, stored in its meta sub-map at key 20 -> 24 -> 25.
/// Used to recover blocks (Amp/Cab) that have no named entry elsewhere.
fn chain_block_model_id(block: &Value) -> Option<u64> {
    as_map(block)
        .and_then(|m| map_get(m, 20)).and_then(as_map)
        .and_then(|m| map_get(m, 24)).and_then(as_map)
        .and_then(|m| map_get(m, 25)).and_then(|v| v.as_u64())
}

fn value_to_param(v: &Value) -> ParamValue {
    match v {
        Value::Boolean(b) => ParamValue::Bool(*b),
        Value::F32(f) => ParamValue::Float(*f),
        Value::F64(f) => ParamValue::Float(*f as f32),
        Value::Integer(i) => ParamValue::Float(i.as_f64().unwrap_or(0.0) as f32),
        _ => ParamValue::Raw(vec![]),
    }
}

/// Recursively walk the decoded preset, collecting every sub-map that looks
/// like a module entry (has a string name, an enabled flag and a slot index).
fn collect_modules(value: &Value, out: &mut Vec<ModuleInfo>) {
    match value {
        Value::Map(entries) => {
            let is_module = map_get(entries, KEY_NAME).and_then(|v| v.as_str()).is_some()
                && map_get(entries, KEY_SLOT).and_then(|v| v.as_u64()).is_some()
                && map_get(entries, KEY_ENABLED).is_some();
            if is_module {
                if let Some(m) = module_from_map(entries) {
                    out.push(m);
                }
            }
            for (_, v) in entries {
                collect_modules(v, out);
            }
        }
        Value::Array(items) => {
            for v in items {
                collect_modules(v, out);
            }
        }
        _ => {}
    }
}

fn module_from_map(entries: &[(Value, Value)]) -> Option<ModuleInfo> {
    let name = map_get(entries, KEY_NAME)?
        .as_str()?
        .trim_end_matches('\0')
        .to_string();
    if name.is_empty() || name.starts_with("SNAPSHOT") {
        return None;
    }
    let slot = map_get(entries, KEY_SLOT).and_then(|v| v.as_u64()).unwrap_or(0) as u8;
    // Key 7 is the enabled flag (true = active); a block is bypassed when it is
    // present but not enabled. Default to enabled if the flag is missing.
    let enabled = map_get(entries, KEY_ENABLED)
        .and_then(|v| v.as_bool())
        .unwrap_or(true);
    let bypassed = !enabled;
    let type_id = map_get(entries, KEY_ID)
        .and_then(|v| v.as_u64())
        .map(format_type_id)
        .unwrap_or_default();
    let category = category_for_name(&name).unwrap_or("Unknown").to_string();
    Some(ModuleInfo {
        name,
        category,
        slot,
        bypassed,
        type_id,
        // Parameter values are decoded in a later stage; the chain block float
        // arrays (preset[0][22][..]) are not yet mapped onto the param controls.
        parameters: vec![],
    })
}

/// Reconstruct the MessagePack-byte hex form of an integer id (matching
/// MODULE_DB's key format) for diagnostics / logging.
fn format_type_id(v: u64) -> String {
    if v < 0x80 {
        format!("{:02x}", v)
    } else if v < 0x100 {
        format!("cc{:02x}", v)
    } else if v < 0x10000 {
        format!("cd{:04x}", v)
    } else {
        format!("ce{:08x}", v)
    }
}


pub fn models_by_category(category: &str) -> Vec<&'static str> {
    let mut names: Vec<&'static str> = MODULE_DB
        .iter()
        .filter(|(_, (cat, _))| *cat == category)
        .map(|(_, (_, name))| *name)
        .collect();
    names.sort();
    names
}

pub fn all_amp_models() -> Vec<&'static str> {
    let mut names: Vec<&'static str> = MODULE_DB
        .iter()
        .filter(|(_, (cat, _))| *cat == "Amp" || *cat == "Distortion" || *cat == "Fuzz")
        .map(|(_, (_, name))| *name)
        .collect();
    names.sort();
    names
}

pub fn all_cab_models() -> Vec<&'static str> {
    models_by_category("Cab")
}

pub fn all_effect_models() -> Vec<&'static str> {
    let mut names: Vec<&'static str> = MODULE_DB
        .iter()
        .filter(|(_, (cat, _))| {
            !matches!(*cat, "Amp" | "Cab" | "Input" | "Output" | "Send/Return" | "FX Loop")
        })
        .map(|(_, (_, name))| *name)
        .collect();
    names.sort();
    names
}

/// Whether a parsed block belongs to one of POD Go's fixed (dedicated) block
/// positions — Amp, Cab, Wah, Volume pedal, and the FX Loop — as opposed to
/// one of the four freely-assignable FX slots. Anything not classified here is
/// treated as an assignable FX block.
///
/// NOTE: EQ can be EITHER the dedicated Preset EQ block OR an EQ used in an FX
/// slot; the two are only distinguishable by chain position (TBD on hardware),
/// so EQ is currently NOT treated as fixed here.
pub fn is_fixed_block_category(category: &str, name: &str) -> bool {
    match category {
        "Amp" | "Cab" | "Wah" | "Vol/Pan" => true,
        // EQ is POD Go's dedicated Preset EQ block. (An EQ could in principle
        // also be dropped into an FX slot; in that rare case it would route to
        // Preset EQ here. POD Go has only one Preset EQ block.)
        "EQ" => true,
        "Send/Return" => true,
        _ => name.contains("FX Loop"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_fixed_blocks() {
        assert!(is_fixed_block_category("Wah", "Weeper"));
        assert!(is_fixed_block_category("Vol/Pan", "Volume Pedal"));
        assert!(is_fixed_block_category("Unknown", "Mono FX Loop"));
        assert!(is_fixed_block_category("EQ", "10 Band Graphic")); // Preset EQ
        assert!(!is_fixed_block_category("Distortion", "Deez One Vintage"));
        assert!(!is_fixed_block_category("Modulation", "Gray Flanger"));
    }

    // Integration check against a real captured preset, when present. Skips on
    // machines without the capture (e.g. CI). Capture via PODGO_DUMP=1.
    #[test]
    fn parses_amp_cab_from_captured_preset() {
        let path = "/tmp/podgo_preset.bin";
        if !std::path::Path::new(path).exists() {
            eprintln!("skipping: {path} not present");
            return;
        }
        let data = std::fs::read(path).unwrap();
        let preset = parse_preset_data(&data);
        let has = |cat: &str| preset.modules.iter().any(|m| m.category == cat);
        assert!(has("Amp"), "expected an Amp block to be recovered from chain meta");
        assert!(has("Cab"), "expected a Cab block to be recovered from chain meta");
    }
}
