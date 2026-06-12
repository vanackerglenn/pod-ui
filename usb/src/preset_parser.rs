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

// === Binary Parser ===

pub fn parse_preset_data(data: &[u8]) -> PresetData {
    let mut preset = PresetData::default();

    // Find slot sections after 8215 marker
    if let Some(slot_start) = find_marker(data, &[0x82, 0x15]) {
        let slot_data_start = slot_start + 14; // skip 14 bytes after 8215
        let slot_data_end = find_marker(data, &[0x08, 0x95])
            .unwrap_or(data.len());
        if slot_data_start < slot_data_end {
            let slot_region = &data[slot_data_start..slot_data_end];
            parse_slot_modules(slot_region, &mut preset);
        }
    }

    // Parse footswitch data after 0895 marker
    if let Some(fs_start) = find_marker(data, &[0x08, 0x95]) {
        let fs_data = &data[fs_start..];
        parse_footswitch_data(fs_data, &mut preset);
    }

    // Deduplicate modules by name+slot
    preset.modules.sort_by(|a, b| a.slot.cmp(&b.slot));
    preset.modules.dedup_by(|a, b| a.name == b.name && a.slot == b.slot);

    preset
}

fn find_marker(data: &[u8], marker: &[u8]) -> Option<usize> {
    data.windows(marker.len()).position(|w| w == marker)
}

fn parse_slot_modules(data: &[u8], preset: &mut PresetData) {
    // Find all 91 87 module entry markers in the slot region
    let mut pos = 0;
    while pos < data.len() {
        match data[pos..].windows(2).position(|w| w == [0x91, 0x87]) {
            Some(off) => {
                let start = pos + off;
                if let Some(m) = parse_single_module(&data[start..]) {
                    preset.modules.push(m);
                }
                pos = start + 2;
            }
            None => break,
        }
    }
}

fn parse_single_module(data: &[u8]) -> Option<ModuleInfo> {
    if data.len() < 4 { return None; }

    let mut pos = 0;
    let mut name: Option<String> = None;
    let mut type_id: Option<String> = None;
    let mut bypassed: Option<bool> = None;
    let mut slot: Option<u8> = None;
    let mut parameters: Vec<ParamValue> = vec![];

    // Skip the 91 87 marker
    if data[pos] == 0x91 && data.get(pos+1) == Some(&0x87) {
        pos += 2;
    } else {
        return None;
    }

    // Skip header bytes (typically: 0a 00 0b 85 00 01)
    if data.get(pos) == Some(&0x0a) {
        pos += 1;
        // skip value bytes for key 0x0a
        if data.get(pos) == Some(&0x00) { pos += 1; }
        // key 0x0b
        if data.get(pos) == Some(&0x0b) { pos += 1; skip_raw_bytes(data, &mut pos, 3); }
    }

    // Read key-value pairs until we hit another module or end
    while pos < data.len() {
        // Check if we hit next module marker
        if data[pos] == 0x91 && data.get(pos+1) == Some(&0x87) {
            break;
        }
        // Check for 0x82 0x15 section boundary
        if data[pos] == 0x82 && data.get(pos+1) == Some(&0x15) {
            break;
        }

        let key = data[pos];
        pos += 1;

        match key {
            0x05 => {
                // Module name (fixstr)
                if let Some(s) = read_string(data, &mut pos) {
                    name = Some(s);
                }
            }
            0x06 => {
                // Type ID or parameter value
                // ID format: ce + 4 bytes (uint32), cd/cc + 2 bytes (uint16), or single byte
                let id = if let Some(&0xce) = data.get(pos) {
                    pos += 1;
                    let v = read_u32(data, &mut pos);
                    format!("ce{:08x}", v)
                } else if let Some(&0xcd) = data.get(pos) {
                    pos += 1;
                    let v = read_u16(data, &mut pos);
                    format!("cd{:04x}", v)
                } else if let Some(&0xcc) = data.get(pos) {
                    pos += 1;
                    let v = read_u16(data, &mut pos);
                    format!("cc{:04x}", v)
                } else {
                    let v = read_byte(data, &mut pos);
                    format!("{:02x}", v)
                };
                // Try as type ID first
                if MODULE_DB.contains_key(id.as_str()) {
                    type_id = Some(id);
                } else {
                    // It's a parameter value
                    // Parse the value type
                    let val = read_raw_value(data, &mut pos);
                    parameters.push(val);
                }
            }
            0x07 => {
                // Bypass state (c2=false, c3=true)
                if let Some(b) = read_bool(data, &mut pos) {
                    bypassed = Some(b);
                }
            }
            0x08 => {
                // Slot index
                slot = Some(read_byte(data, &mut pos));
            }
            0x0c => {
                // Additional state (c2|c3 or value)
                let _ = read_raw_value(data, &mut pos);
            }
            0x0d => {
                // Flag
                let _ = read_bool(data, &mut pos);
            }
            0x0e => {
                // String (custom label)
                let _ = read_string(data, &mut pos);
            }
            0x10 => {
                // LED color
                let _ = read_byte(data, &mut pos);
            }
            0x0f => {
                // Another flag
                let _ = read_bool(data, &mut pos);
            }
            _ => {
                // Unknown key — skip its value
                if key >= 0xa0 && key <= 0xbf {
                    // This is actually a string (fixstr), key was consumed as string prefix
                    // Let's handle this: put the "key" back as string prefix
                    // The key is actually the string length byte
                    let slen = (((key & 0xf0) - 0xa0) + (key & 0x0f)) as usize;
                    if pos + slen <= data.len() {
                        let s: String = data[pos..pos+slen].iter()
                            .take_while(|&&c| c != 0)
                            .map(|&c| c as char)
                            .collect();
                        if name.is_none() && !s.is_empty() && s.len() > 1 && !s.starts_with("SNAPSHOT") {
                            name = Some(s);
                        }
                        pos += slen;
                        if data.get(pos) == Some(&0x00) { pos += 1; }
                    }
                    break;
                }
                skip_unexpected(data, &mut pos);
            }
        }
    }

    let name = name.unwrap_or_else(|| "Unknown".to_string());
    let type_id = type_id.unwrap_or_else(|| String::new());
    let category = MODULE_DB.get(type_id.as_str())
        .map(|(c, _)| *c)
        .unwrap_or("Unknown")
        .to_string();

    Some(ModuleInfo {
        name,
        category,
        slot: slot.unwrap_or(0),
        bypassed: bypassed.unwrap_or(false),
        type_id,
        parameters,
    })
}

fn read_byte(data: &[u8], pos: &mut usize) -> u8 {
    let v = data.get(*pos).copied().unwrap_or(0);
    *pos += 1;
    v
}

fn read_u16(data: &[u8], pos: &mut usize) -> u16 {
    let v = data.get(*pos).copied().unwrap_or(0) as u16;
    let v2 = data.get(*pos + 1).copied().unwrap_or(0) as u16;
    *pos += 2;
    (v << 8) | v2
}

fn read_u32(data: &[u8], pos: &mut usize) -> u32 {
    let v = data.get(*pos).copied().unwrap_or(0) as u32;
    let v2 = data.get(*pos + 1).copied().unwrap_or(0) as u32;
    let v3 = data.get(*pos + 2).copied().unwrap_or(0) as u32;
    let v4 = data.get(*pos + 3).copied().unwrap_or(0) as u32;
    *pos += 4;
    (v << 24) | (v2 << 16) | (v3 << 8) | v4
}

fn read_bool(data: &[u8], pos: &mut usize) -> Option<bool> {
    match data.get(*pos) {
        Some(0xc2) => { *pos += 1; Some(true) }
        Some(0xc3) => { *pos += 1; Some(false) }
        _ => None,
    }
}

fn read_string(data: &[u8], pos: &mut usize) -> Option<String> {
    let len_byte = data.get(*pos)?;
    if *len_byte >= 0xa0 && *len_byte <= 0xbf {
        let slen = (((len_byte & 0xf0) - 0xa0) + (len_byte & 0x0f)) as usize;
        *pos += 1;
        if *pos + slen <= data.len() {
            let end = data[*pos..].iter().position(|&c| c == 0).unwrap_or(slen).min(slen);
            let s: String = data[*pos..*pos+end].iter().map(|&c| c as char).collect();
            *pos += end;
            // Skip null terminator
            if *pos < data.len() && data[*pos] == 0x00 { *pos += 1; }
            // Skip remaining of slen if we stopped early
            *pos += slen - end;
            return Some(s);
        }
        *pos += slen.min(data.len().saturating_sub(*pos));
    }
    None
}

fn read_raw_value(data: &[u8], pos: &mut usize) -> ParamValue {
    // Check if it's a float (ca + 4 bytes)
    if data.get(*pos) == Some(&0xca) {
        *pos += 1;
        if *pos + 4 <= data.len() {
            let bytes: [u8; 4] = [
                data[*pos],
                data[*pos + 1],
                data[*pos + 2],
                data[*pos + 3],
            ];
            *pos += 4;
            return ParamValue::Float(f32::from_be_bytes(bytes));
        }
        return ParamValue::Int(0);
    }
    // Check if it's a bool
    if let Some(b) = read_bool(data, pos) {
        return ParamValue::Bool(b);
    }
    // Default: single byte int
    let v = read_byte(data, pos);
    ParamValue::Int(v)
}

fn skip_raw_bytes(data: &[u8], pos: &mut usize, n: usize) {
    *pos += n.min(data.len().saturating_sub(*pos));
}

fn skip_unexpected(data: &[u8], pos: &mut usize) {
    // Skip a byte and try to resync
    *pos += 1;
}

fn parse_footswitch_data(data: &[u8], preset: &mut PresetData) {
    // Find footswitch sections after 0895, before 049a
    let start = 2; // skip 0895
    let end = find_marker(data, &[0x04, 0x9a]).unwrap_or(data.len());
    if start >= end { return; }
    let fs_region = &data[start..end];

    let mut pos = 0;
    while pos < fs_region.len() {
        // Look for 9X 87 markers
        if fs_region[pos] >= 0x90 && fs_region[pos] <= 0x9f
            && fs_region.get(pos + 1) == Some(&0x87) {
            let _count = (fs_region[pos] & 0x0f) as usize;
            pos += 2;
            // Parse children
            let mut info = FootSwitchInfo {
                label: String::new(),
                custom_label: String::new(),
                led_color: -1,
            };
            parse_fs_children(fs_region, &mut pos, &mut info);
            if !info.label.is_empty() || !info.custom_label.is_empty() {
                preset.footswitches.push(info);
            }
        } else if fs_region[pos] == 0xc0 {
            // Empty slot
            pos += 1;
        } else {
            pos += 1;
        }
    }
}

fn parse_fs_children(data: &[u8], pos: &mut usize, info: &mut FootSwitchInfo) {
    while *pos < data.len() {
        if data[*pos] >= 0x90 && data.get(*pos + 1) == Some(&0x87) {
            break; // next section
        }
        if data[*pos] == 0xc0 { break; }

        let marker = data[*pos];
        *pos += 1;

        match marker {
            0x05 => {
                if let Some(s) = read_string(data, pos) {
                    info.label = s;
                }
            }
            0x0e => {
                if let Some(s) = read_string(data, pos) {
                    info.custom_label = s;
                }
            }
            0x10 => {
                info.led_color = read_byte(data, pos) as i32;
            }
            0x07 | 0x0d | 0x0f | 0x29 => {
                let _ = read_bool(data, pos);
            }
            0x0a => {
                let _ = read_byte(data, pos);
                if data.get(*pos) == Some(&0x00) { *pos += 1; }
            }
            0x0b => { skip_raw_bytes(data, pos, 3); }
            0x08 => {
                let _ = read_byte(data, pos);
            }
            0x09 => { skip_raw_bytes(data, pos, 5); }
            _ => {
                // Try to skip a reasonable amount
                if (0xa0..=0xbf).contains(&marker) {
                    // String that we missed the key for
                    let slen = (((marker & 0xf0) - 0xa0) + (marker & 0x0f)) as usize;
                    skip_raw_bytes(data, pos, slen + 1);
                 } else {
                    skip_raw_bytes(data, pos, 1);
                }
            }
        }
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
