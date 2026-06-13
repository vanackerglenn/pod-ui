# POD Go 4-FX-Slot Block Model Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking. NOTE: this is a UI-heavy plan; glade tasks are not classic TDD — they require a human to visually verify on real hardware. Config/handler/parser tasks ARE testable offline against a captured preset (`/tmp/podgo_preset.bin`).

**Goal:** Restructure the POD Go UI/config to match the device's real block model — fixed Amp, Cab, Wah, Volume, Preset EQ, FX Loop blocks plus **four freely-assignable FX slots** — so every block in a loaded preset is displayed without the current single-`stomp_select` conflation.

**Architecture:** POD Go is a **read-only preset viewer**. Presets are read over the USB vendor protocol and parsed (MessagePack) into modules with name/category/bypass/params (already working). The handler assigns each parsed block to the correct UI control by category/slot. The UI mirrors the device's fixed layout: 6 dedicated blocks + 4 generic FX slots. Each FX slot is a model select + enable + a set of param sliders shown/relabeled per selected model.

**Tech Stack:** Rust, GTK3 (glade), `pod_core` controller/wiring, `pod_usb` parser (rmpv MessagePack).

---

## Critical Context (read before starting)

These facts were established by reverse-engineering and the POD Go 2.50 owner's manual (`mod-podgo/src/docs/POD Go 2.50 Owner's Manual - English .pdf`). Do not re-derive them.

1. **Device block model (manual p.248-253):** "POD Go can accommodate one amp/preamp block, one cab/IR block, a Wah block, a Volume pedal block, a Preset EQ block, an FX Loop block, and up to four additional effects blocks, all simultaneously." → 6 fixed blocks + 4 generic FX slots.
2. **POD Go has NO MIDI CC for model/param control** (manual MIDI CC table, p.49). Only EXP pedals (CC1/2), footswitches (CC49-56), looper (CC60-66), tap (CC64), tuner (CC68), snapshot (CC69). Therefore:
   - The UI is **read-only** — editing a control cannot be sent to the device (would require RE'ing vendor-protocol *writes*, out of scope here).
   - The per-control `cc`/`addr` values in `mod-podgo/src/config.rs` are **fictional/inert**; they only serve as unique keys + value holders for display. New controls may use any unique, non-colliding `cc`/`addr`.
3. **Preset parsing works** (`usb/src/preset_parser.rs`, rmpv): yields `ModuleInfo { name, category, slot, bypassed, type_id, parameters }`. `category` resolved by name; `parameters` are the chain block's value array (`root[0][22][slot]`), mostly 0.0-1.0 normalized, some native units (Hz/dB), some the first being a tempo value.
4. **Per-model param specs are NOT available from the device or community DBs** (helix_usb/openhx read positionally). They live in HX Edit. The manual documents *some*. So param NAMES/ORDER/UNITS are hand-mapped incrementally (see Task 7). Param VALUES are already extracted correctly in device order.
5. **Wiring is correct and origin-agnostic:** setting a controller value (Origin::NONE, Signal::Force) updates the bound glade widget via `controller_rx_handler`. `wire_dynamic_select` shows/hides/relabels param widgets when a select changes. See `mod-podgo/src/module.rs`.
6. The glade already contains an unused `module_chain` / `module_chain_scroll` container (leftover from an abandoned programmatic-chain approach). The chosen approach here is the fixed-control model (consistent with other devices), NOT the programmatic chain; the leftover container should be removed in Task 6.

**Alternative considered & rejected:** a programmatically-built read-only chain list (one widget row per block). Rejected because it diverges from how every other device in pod-ui is structured (user prefers consistency) and doesn't reuse the existing wiring/show-hide infrastructure. Revisit only if the 4-slot glade work proves intractable.

---

## File Structure

- `mod-podgo/src/config.rs` — add 4 FX-slot control groups (`fxN_select`, `fxN_enable`, `fxN_paramK`), fixed-block controls (`volume_*`, `preset_eq_*`, `fx_loop_*`), and an `FX_MODELS` list (all assignable effect models). Remove the single `stomp_*` controls once FX slots replace them.
- `mod-podgo/src/model.rs` / `builders.rs` — extend config-builder types so each FX model can carry an ordered, named, optionally-scaled param spec (replacing the current name-only label map).
- `mod-podgo/src/handler.rs` — rewrite `sync_controller_from_preset` to assign FX-category blocks to FX slots (by chain slot order) and fixed blocks to their dedicated controls; rewrite `sync_params` to use each model's param spec.
- `mod-podgo/src/module.rs` — wire the new controls; `wire_dynamic_select` for each FX slot.
- `mod-podgo/src/pod-go.glade` — add 4 FX-slot widget groups + Volume/Preset EQ/FX Loop blocks; remove `module_chain*` and the old single `stomp_*` widgets.
- `usb/src/preset_parser.rs` — already attaches params; add a stable way to expose, per module, whether it is one of the (up to 4) FX blocks vs a fixed block (helper `is_fixed_block_category`).
- `usb/examples/podgo_parse_check.rs` — extend to print slot→control assignment for offline verification.

---

### Task 1: Define the FX model param-spec type

**Files:**
- Modify: `mod-podgo/src/model.rs`
- Modify: `mod-podgo/src/builders.rs`
- Test: `mod-podgo/src/model.rs` (`#[cfg(test)]`)

- [ ] **Step 1: Write the failing test**

```rust
// in mod-podgo/src/model.rs
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
```

- [ ] **Step 2: Run test, verify it fails**

Run: `cargo test -p pod-mod-podgo param_spec_indexes_named_params 2>&1`
Expected: FAIL (`ParamSpec` not found).

- [ ] **Step 3: Implement `ParamSpec`**

```rust
// in mod-podgo/src/model.rs
#[derive(Clone, Debug, Default)]
pub struct ParamSpec {
    /// Ordered param labels as the device lists them. Empty string = a param
    /// position that exists but is not surfaced in the UI yet.
    labels: Vec<String>,
}

impl ParamSpec {
    pub fn new(labels: &[&str]) -> Self {
        ParamSpec { labels: labels.iter().map(|s| s.to_string()).collect() }
    }
    pub fn label(&self, idx: usize) -> Option<&str> {
        self.labels.get(idx).map(|s| s.as_str()).filter(|s| !s.is_empty())
    }
    pub fn len(&self) -> usize { self.labels.len() }
}
```

- [ ] **Step 4: Run test, verify it passes**

Run: `cargo test -p pod-mod-podgo param_spec_indexes_named_params 2>&1`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add mod-podgo/src/model.rs
git commit -m "feat(pod-go): add ParamSpec type for ordered per-model params"
```

---

### Task 2: FX model catalog with param specs

**Files:**
- Modify: `mod-podgo/src/config.rs`
- Test: `mod-podgo/src/config.rs` (`#[cfg(test)]`)

Build `FX_MODELS: Vec<FxModel>` where `FxModel { name: String, category: &'static str, params: ParamSpec }`, seeded from `pod_usb::models_by_category(..)` for every assignable effect category (Distortion, Distortion (Legacy), Dynamic, EQ, Modulation, Delay, Reverb, Pitch/Synth, Filter, Wah, Vol/Pan). Param specs start empty (generic) and are filled in Task 7 for known models.

- [ ] **Step 1: Write the failing test**

```rust
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
```

- [ ] **Step 2: Run test, verify it fails**

Run: `cargo test -p pod-mod-podgo fx_models_includes_known_effects 2>&1`
Expected: FAIL (`FX_MODELS` not found).

- [ ] **Step 3: Implement `FxModel` + `FX_MODELS`**

```rust
// in mod-podgo/src/config.rs
use crate::model::ParamSpec;

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
    for cat in FX_CATEGORIES {
        for name in pod_usb::models_by_category(cat) {
            v.push(FxModel {
                name: name.to_string(),
                category: cat,
                params: param_spec_for(name), // Task 7 fills these; default empty
            });
        }
    }
    v
});

// Task 7 grows this; default is an empty (generic) spec.
fn param_spec_for(_name: &str) -> ParamSpec { ParamSpec::default() }
```

- [ ] **Step 4: Run test, verify it passes**

Run: `cargo test -p pod-mod-podgo fx_models_includes_known_effects 2>&1`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add mod-podgo/src/config.rs
git commit -m "feat(pod-go): add FX_MODELS catalog for assignable FX slots"
```

---

### Task 3: Add FX-slot + fixed-block controls to CONFIG

**Files:**
- Modify: `mod-podgo/src/config.rs` (the `CONFIG` `controls` map + `init_controls`)

Add, for `N` in 1..=4: `fxN_select` (Select), `fxN_enable` (SwitchControl), `fxN_param1`..`fxN_param8` (RangeControl, `fmt_percent`). Add fixed blocks not yet modelled: `volume_enable`/`volume_position`, `preset_eq_enable` + `preset_eq_param1..6`, `fx_loop_enable`/`fx_loop_mix`. Use arbitrary unique `cc`/`addr` values (≥ any existing; remember they are inert — see Critical Context #2). Keep existing `amp_*`, `cab_*`, `wah_*`. **Remove** the old `stomp_*` controls (replaced by FX slots) and `mod_*`/`delay_*`/`reverb_*` *select/param* controls **only after** Task 5 routes those categories through the FX slots — to avoid a broken intermediate, do the removal in Task 6.

- [ ] **Step 1: Add the controls** (example for one FX slot; repeat for 1..=4)

```rust
// inside the convert_args!(hashmap!( ... )) of CONFIG.controls
"fx1_select" => Select { cc: 120, addr: 32 + 120, ..def() },
"fx1_enable" => SwitchControl { cc: 121, addr: 32 + 121, ..def() },
"fx1_param1" => RangeControl { cc: 122, addr: 32 + 122, format: fmt_percent!(), ..def() },
// ... fx1_param2..8 with subsequent unique cc/addr ...
```

NOTE: `program_size` in `CONFIG` is `72*2+16 = 160`; `addr` must stay `< 160`. Since these are inert for read-only display, prefer NOT using `addr` at all — set controls without `addr` (the buffer path is never exercised for POD Go). Confirm `Select`/`RangeControl`/`SwitchControl` allow a no-addr form (`..def()` with `addr` defaulted); if `addr` is mandatory, widen `program_size` to fit all new controls and assign sequential addrs.

- [ ] **Step 2: Verify compilation**

Run: `cargo build -p pod-mod-podgo 2>&1`
Expected: no errors.

- [ ] **Step 3: Commit**

```bash
git add mod-podgo/src/config.rs
git commit -m "feat(pod-go): add 4 FX-slot and fixed-block controls to CONFIG"
```

---

### Task 4: Parser — classify fixed vs FX blocks

**Files:**
- Modify: `usb/src/preset_parser.rs`
- Test: `usb/src/preset_parser.rs` (`#[cfg(test)]` reading `/tmp/podgo_preset.bin` guarded by file existence)

Add `pub fn is_fixed_block_category(category: &str, name: &str) -> bool` returning true for Amp, Cab, Wah, Vol/Pan, and the Preset-EQ + FX-Loop blocks (FX Loop is `category == "Unknown"` with name containing "FX Loop"; Preset EQ is the EQ block — but EQ can ALSO be an FX block, so Preset EQ must be distinguished by slot/position, TBD during hardware verification — default: treat EQ as FX). Everything else assignable is an FX block.

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn classifies_fixed_blocks() {
    assert!(is_fixed_block_category("Wah", "Weeper"));
    assert!(is_fixed_block_category("Vol/Pan", "Volume Pedal"));
    assert!(is_fixed_block_category("Unknown", "Mono FX Loop"));
    assert!(!is_fixed_block_category("Distortion", "Deez One Vintage"));
    assert!(!is_fixed_block_category("Modulation", "Gray Flanger"));
}
```

- [ ] **Step 2: Run, verify fail.** `cargo test -p pod-usb classifies_fixed_blocks` → FAIL.

- [ ] **Step 3: Implement**

```rust
pub fn is_fixed_block_category(category: &str, name: &str) -> bool {
    match category {
        "Amp" | "Cab" | "Wah" | "Vol/Pan" => true,
        "Send/Return" => true,
        _ => name.contains("FX Loop"),
    }
}
```

- [ ] **Step 4: Run, verify pass.** Commit.

```bash
git add usb/src/preset_parser.rs
git commit -m "feat(pod-go): classify fixed vs assignable FX blocks"
```

---

### Task 5: Handler — assign blocks to FX slots and fixed controls

**Files:**
- Modify: `mod-podgo/src/handler.rs`
- Modify: `usb/examples/podgo_parse_check.rs` (print the slot assignment)

Rewrite `sync_controller_from_preset`: (1) `reset_managed_blocks` clears all 4 FX slots + fixed blocks. (2) Iterate preset modules sorted by `slot`; route fixed blocks (Wah→`wah_select`, Vol/Pan→`volume_*`, FX Loop→`fx_loop_*`, Amp→`amp_select`, Cab→`cab_select`) to dedicated controls. (3) Collect non-fixed (FX) blocks in slot order and assign the first four to `fx1..fx4` (`fxN_select` = `FX_MODELS.position(name)`, `fxN_enable` = `!bypassed`, params via `sync_fx_params`). (4) `sync_fx_params` maps the model's `ParamSpec` labels to `fxN_paramK` (0..=1 float → ×127, bool → 0/127, else skip).

- [ ] **Step 1: Extend the offline check to print routing** (`podgo_parse_check.rs`): for each module print whether it's a fixed block or `FX slot k`. (No device needed.)

- [ ] **Step 2: Implement the new `sync_controller_from_preset` + `sync_fx_params`** (full code; mirror current handler patterns, using `FX_MODELS` and `set_select`/`store_set`).

- [ ] **Step 3: Offline-validate**

Run: `cargo run -p pod-usb --example podgo_parse_check -- /tmp/podgo_preset.bin 2>&1 | grep -E "slot|FX slot|fixed"`
Expected: Weeper→fixed(wah), Volume Pedal→fixed(volume), Mono FX Loop→fixed(fxloop); Pitch Wham→FX slot 1, Deez One→FX slot 2, 10 Band Graphic→FX slot 3, Gray Flanger→FX slot 4 (Chamber would be slot 5 → log "more than 4 FX blocks; <name> not shown"). Adjust ordering rule if hardware shows a different slot mapping.

- [ ] **Step 4: Build full workspace.** `cargo build 2>&1` → no errors. Commit.

```bash
git add mod-podgo/src/handler.rs usb/examples/podgo_parse_check.rs
git commit -m "feat(pod-go): route preset blocks to 4 FX slots + fixed controls"
```

---

### Task 6: Glade — 4 FX-slot widgets + fixed blocks; remove legacy

**Files:**
- Modify: `mod-podgo/src/pod-go.glade`
- Modify: `mod-podgo/src/config.rs` (remove old `stomp_*`/`mod_*`/`delay_*`/`reverb_*` select+param controls now superseded)
- Modify: `mod-podgo/src/module.rs` (drop references to removed widgets)

**This task requires visual verification on hardware — not TDD.** Work incrementally, building after each slot.

- [ ] **Step 1:** Remove the unused `module_chain` and `module_chain_scroll` widgets from the glade.
- [ ] **Step 2:** For each FX slot `N` in 1..=4, add a widget group containing: a `GtkComboBoxText` id `fxN_select`, a `GtkCheckButton` id `fxN_enable`, and 8 `GtkScale` ids `fxN_param1..8` each paired with a `GtkLabel` id `fxN_paramK_label`. Duplicate the existing `stomp_*` group's markup as the template (it already has select + param + param_label widgets).
- [ ] **Step 3:** Add fixed-block widgets: `volume_enable`+`volume_position`, `preset_eq_enable`+`preset_eq_param1..6`(+labels), `fx_loop_enable`+`fx_loop_mix`.
- [ ] **Step 4:** Remove the legacy single `stomp_*` widgets and the now-superseded `mod_*`/`delay_*`/`reverb_*` *select/param* widgets (the FX slots replace them). Remove the matching controls from `config.rs` and any `module.rs` wiring referencing them.
- [ ] **Step 5:** `cargo build 2>&1` → no errors; `cargo run -p pod-gui` and load a preset on the POD Go; visually confirm the 4 FX slots + fixed blocks render.
- [ ] **Step 6:** Commit.

```bash
git add mod-podgo/src/pod-go.glade mod-podgo/src/config.rs mod-podgo/src/module.rs
git commit -m "feat(pod-go): 4 FX-slot + fixed-block UI; remove legacy single-slot widgets"
```

---

### Task 7: Wiring + dynamic params + per-model specs

**Files:**
- Modify: `mod-podgo/src/module.rs` (wire each `fxN_select` with `wire_dynamic_select`; `init_combo` over `FX_MODELS`)
- Modify: `mod-podgo/src/config.rs` (`param_spec_for` — fill known models)

- [ ] **Step 1:** In `module.rs::wire`, `init_combo` each `fxN_select` over `FX_MODELS` (display `m.name`), call `pod_gtk::wire`, then `wire_dynamic_select("fxN_select", &FX_MODELS, ...)` so selecting a model shows/relabels that model's `fxN_paramK` widgets per its `ParamSpec`. Build; `cargo run -p pod-gui`; verify on hardware that the loaded preset's FX models + their param sliders appear with correct values.
- [ ] **Step 2:** Grow `param_spec_for` with verified per-model specs, e.g.:

```rust
fn param_spec_for(name: &str) -> ParamSpec {
    match name {
        "Deez One Vintage" => ParamSpec::new(&["Gain", "Tone", "Level"]), // device calls it "Gain"
        "Gray Flanger"     => ParamSpec::new(&["Rate", "Width", "Manual", "Regen", "Spread", "Mix", "Level", "Headroom"]),
        _ => ParamSpec::default(),
    }
}
```

Source names from the owner's manual where documented; otherwise capture from the device screen during testing. Verify each against hardware (param order, names, and whether the first param is a tempo value rather than a percent — those need a non-percent display, deferred).

- [ ] **Step 3:** Commit per verified batch of models.

```bash
git add mod-podgo/src/module.rs mod-podgo/src/config.rs
git commit -m "feat(pod-go): wire FX slots, dynamic params, per-model specs"
```

---

## Out of scope / future

- **Writing edits back to the device** — POD Go has no MIDI CC for params (Critical Context #2); would require reverse-engineering vendor-protocol *writes*. Until then the UI is read-only.
- **Tempo-synced params** (note divisions like "1/4") and **native-unit params** (Hz/dB) need dedicated control formats rather than the percent sliders.
- **Preset EQ vs an EQ used as an FX block** — disambiguation may need slot/position logic confirmed on hardware (Task 4 note).
- **Distinguishing the four FX slots' canonical order** — current rule is "FX-category blocks in chain-slot order"; confirm against how the device numbers FX1-FX4.

## Self-review notes
- Spec coverage: block model (Tasks 3,5,6), FX slots (2,3,5,6,7), fixed blocks (3,5,6), params (1,5,7), read-only framing (Critical Context). Covered.
- Types are consistent: `ParamSpec` (Task 1) used by `FxModel`/`FX_MODELS` (Task 2) and `param_spec_for` (Tasks 2,7) and handler `sync_fx_params` (Task 5).
- Known risk: glade tasks (6,7) are not offline-verifiable and depend on hardware iteration with the user.
