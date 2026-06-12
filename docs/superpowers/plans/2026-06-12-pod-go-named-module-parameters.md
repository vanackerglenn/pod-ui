# Pod Go Named Module Parameters Display Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add human-readable parameter names for each effect type in the Pod Go module chain UI.

**Architecture:** Define static config arrays (STOMP_CONFIG, MOD_CONFIG, DELAY_CONFIG) in config.rs using existing builder pattern, then look up effect names in module.rs to display named labels instead of generic "param[i]".

**Tech Stack:** Rust, GTK, pod_usb

---

### Task 1: Add STOMP_CONFIG array to config.rs

**Files:**
- Modify: `mod-podgo/src/config.rs` (after line 44, before CONFIG)

- [ ] **Step 1: Add STOMP_CONFIG static array**

Add after the CAB_MODELS block (around line 44):

```rust
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
```

Make sure to import `stomp` at the top (it's from `crate::builders` which is already imported).

- [ ] **Step 2: Verify compilation**

Run: `cargo check -p mod-podgo 2>&1`
Expected: No errors

- [ ] **Step 3: Commit**

```bash
git add mod-podgo/src/config.rs
git commit -m "feat(pod-go): add STOMP_CONFIG with parameter labels"
```

---

### Task 2: Add MOD_CONFIG and DELAY_CONFIG arrays to config.rs

**Files:**
- Modify: `mod-podgo/src/config.rs` (after STOMP_CONFIG, before CONFIG)

- [ ] **Step 1: Add MOD_CONFIG array**

```rust
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
```

- [ ] **Step 2: Add DELAY_CONFIG array**

```rust
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
```

- [ ] **Step 3: Verify compilation**

Run: `cargo check -p mod-podgo 2>&1`
Expected: No errors

- [ ] **Step 4: Commit**

```bash
git add mod-podgo/src/config.rs
git commit -m "feat(pod-go): add MOD_CONFIG and DELAY_CONFIG with parameter labels"
```

---

### Task 3: Export config arrays and types publicly

**Files:**
- Modify: `mod-podgo/src/config.rs` (add `pub use` or ensure `pub` on configs)
- Check: `mod-podgo/src/lib.rs` (verify config module is public)

The configs (STOMP_CONFIG, MOD_CONFIG, DELAY_CONFIG) are already `pub static` in config.rs. The types (StompConfig, ModConfig, DelayConfig) need to be accessible from module.rs.

- [ ] **Step 1: Verify config module is public in lib.rs**

Check: `mod-podgo/src/lib.rs` — config should be `pub mod config;` or similar. If it's `mod config;`, change to `pub mod config;`.

- [ ] **Step 2: Commit (if changed)**

```bash
git add mod-podgo/src/lib.rs
git commit -m "chore(pod-go): export config module publicly"
```

---

### Task 4: Update module.rs to use named parameters

**Files:**
- Modify: `mod-podgo/src/module.rs` (update `build_module_widget()`)

- [ ] **Step 1: Add import for config arrays**

Add at the top of module.rs (after existing imports):
```rust
use crate::config::{STOMP_CONFIG, MOD_CONFIG, DELAY_CONFIG};
```

- [ ] **Step 2: Add helper function for label lookup**

Add before `build_module_widget()`:

```rust
fn get_param_labels<'a>(module: &pod_usb::ModuleInfo) -> Option<&'a std::collections::HashMap<String, String>> {
    let config: Option<&dyn ConfigAccess> = match module.category.as_str() {
        "Distortion" | "Distortion (Legacy)" | "Dynamic" | "EQ" | "Filter" | "Wah" | "Pitch/Synth" | "Vol/Pan" => {
            STOMP_CONFIG.iter().find(|c| c.name == module.name).map(|c| c as &dyn ConfigAccess)
        }
        "Modulation" => {
            MOD_CONFIG.iter().find(|c| c.name == module.name).map(|c| c as &dyn ConfigAccess)
        }
        "Delay" => {
            DELAY_CONFIG.iter().find(|c| c.name == module.name).map(|c| c as &dyn ConfigAccess)
        }
        _ => None,
    };
    config.map(|c| c.labels())
}
```

Note: The `ConfigAccess` trait is already defined in `crate::model`. We need to import it too.

- [ ] **Step 3: Update build_module_widget() parameter display**

Replace the parameter display loop (lines 77-83) with:

```rust
if !m.parameters.is_empty() {
    let pbox = gtk::Box::new(gtk::Orientation::Vertical, 1);
    pbox.set_margin_start(20);
    pbox.set_margin_end(8);
    pbox.set_margin_top(4);
    pbox.set_margin_bottom(4);

    let labels = get_param_labels(m);
    for (i, p) in m.parameters.iter().enumerate() {
        let pl = Label::new(None);
        pl.set_xalign(0.0);
        let label = labels
            .and_then(|l| l.get(&format!("stomp_param{}", i + 2))
                .or_else(|| l.get(&format!("mod_param{}", i + 2)))
                .or_else(|| l.get(&format!("delay_param{}", i + 2))))
            .map(|s| s.as_str())
            .unwrap_or(&format!("param[{}]", i));
        pl.set_markup(&format!("<span size='small'>{}: {}</span>", label, p));
        pbox.add(&pl);
    }
    expander.add(&pbox);
}
```

- [ ] **Step 4: Add required imports at top of module.rs**

Ensure these imports exist:
```rust
use std::collections::HashMap;
use crate::model::ConfigAccess;
```

- [ ] **Step 5: Verify compilation**

Run: `cargo check -p mod-podgo 2>&1`
Expected: No errors

- [ ] **Step 6: Commit**

```bash
git add mod-podgo/src/module.rs
git commit -m "feat(pod-go): display named parameter labels in module chain"
```

---

### Task 5: Full build check

- [ ] **Step 1: Build full project**

Run: `cargo build 2>&1`
Expected: No errors

- [ ] **Step 2: Run tests if available**

Run: `cargo test 2>&1`
Expected: Pass or no test failures

- [ ] **Step 3: Final commit**

```bash
git add -A
git commit -m "feat(pod-go): add named parameter display for all effect modules"
```
