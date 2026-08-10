# POD Go Edit's model database

These are Line 6's own data files, copied out of the POD Go Edit installation.
They are the schema the device does not send: parameter names, ranges, units,
enum labels, and the model id table. `mod-podgo/src/models_db.rs` embeds and
parses them at startup.

They replaced `module_params.toml`, a hand-authored file that had reached ~1500
lines and was still ~40% unfilled. Nothing here is hand-curated.

| File | What it is | Authoritative for |
|---|---|---|
| `PodGo.sym` | 627 `symbolicID`s. **An entry's array position is the numeric model id the device puts on the wire.** | model identity |
| `*.models` (13 files) | 574 models, 5723 params — each with `name`, `valueType`, DSP `min`/`max`/`default`, `displayType` | parameters |
| `PGControls.json` | 210 `displayType` definitions: units, printf format rules, `dspToDisplayScale`, discrete label lists | units and labels |
| `PGModelCatalog.json` | the model inventory, grouped into the categories the device shows | which models exist, categories |
| `default_preset_p34.hlx` | POD Go Edit's *file* export format (JSON, `@model` = symbolicID) | reference only — **not** the USB format |
| `PodGo 2.50 Owner's Manual.pdf` (in `src/docs/`) | the manual | human reference |

## How they join up

```
chain position → model_id → PodGo.sym[model_id] → symbolicID → *.models entry
                                                                 ├─ params (name, valueType, min/max)
                                                                 └─ displayType → PGControls.json
                                                                                   ├─ dspToDisplayScale
                                                                                   ├─ format / formatUnits
                                                                                   └─ discrete labels
```

`symbolicID` is the join key throughout, and it's consistent across all four
files. Display names are **not** a usable key — see `podgo-preset-format.md`.

## Gotchas

- **`PodGo.sym` position is the id.** This is the single most important fact in
  here, and it is implicit — nothing in the file says so.
- **54 of 627 symbols have no `.models` entry** (`SendMono1-4`, `ReturnMono1-4`,
  extra FX loops). They resolve to an id but have no parameter data.
- **The catalog and the model files name things differently.** Catalog:
  "6 Sw Mono Looper", "Stereo FX Loop". Model files: "6 Switch Looper Mono",
  "FX Loop 1/2". Presets carry the catalog's spelling. Join by id, not name.
- **Amp/Preamp and Cab/Cab-IR are near-duplicate name lists** for different
  block types — 104 of 106 amp names recur as preamps, 28 of 41 cab names recur
  as mic'd IR cabs, with different parameter lists.
- **These files predate current firmware.** They're dated Jan 2025; a newer
  device may report ids they don't cover. Such a model logs a warning and shows
  as an empty position rather than resolving to something wrong.

## Licensing

Line 6's proprietary data, redistributed here for interoperability. If that
matters for how this project is distributed, generate a derived asset at build
time and keep the originals out of the repository.
