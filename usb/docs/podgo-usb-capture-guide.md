# POD Go USB capture guide (Windows + Wireshark/USBPcap)

Goal: capture the USB traffic between **POD Go Edit** (Windows/Mac only) and the
device so we can reverse-engineer the **write** protocol (param edits, bypass,
model changes, store) and the **value encoding** (0..1 vs native Hz/dB/ms).

The read protocol is already understood (`usb/src/current_preset.rs`,
`usb/docs/hx-protocol.md`). The device is `VID 0x0E41 / PID 0x4247`, interface 0
(Vendor Specific) carries the preset protocol on bulk endpoints **0x01 (OUT)**
and **0x81 (IN)**. (Interface 4 is MIDI; interfaces 2/3 are USB audio.)

> Note: param **names / enum options** are NOT on the wire (they live in HX Edit's
> data) — keep hand-filling `mod-podgo/module_params.toml`. Captures give us the
> write commands and the value encoding.

## 1. One-time setup (Windows)
1. Install **Wireshark** (https://www.wireshark.org/). During install, **enable
   USBPcap** (the "USB capture" component) and reboot if asked.
2. Plug the POD Go into the Windows machine via USB. Launch **POD Go Edit** and
   confirm it connects (you can see presets). Then **close** POD Go Edit so we
   capture the connection from the start.
3. (Reduce noise) In Windows *Sound settings*, set playback/recording to NOT use
   the POD Go while capturing, so the audio interface doesn't flood the capture.

## 2. Start a capture
1. Open Wireshark → in the interface list, double-click a **USBPcap** interface
   (USBPcap1, USBPcap2, …). If you don't know which, pick one; the POD Go is on
   whichever root hub it's plugged into. You can tell you've got the right one
   once POD Go Edit traffic appears.
2. Apply a display filter to focus on the vendor protocol:
   ```
   usb.endpoint_address == 0x01 || usb.endpoint_address == 0x81
   ```
   (If a capture looks empty for an action, remove the filter — the write path
   might use another endpoint, and we want to see it.)

## 3. Capture procedure — one isolated action per file
For each interaction below: **start a fresh capture, perform exactly ONE action,
stop, and save** as `NN-description.pcapng`. Keep them small and labelled, and
**write down exactly what you did** (which block, which param, from→to value) —
that lets us diff precisely.

Priority order (most useful first — the early ones are the smallest, cleanest
diffs and unlock the most):

1. **`01-connect.pcapng`** — launch POD Go Edit, let it connect and load the
   current patch, then stop. (Baseline handshake; confirms the capture works.)
2. **`02-bypass-toggle.pcapng`** — with a patch loaded, toggle **one** block's
   on/off (bypass) once. Note which block. *(Simplest possible write — the key
   to cracking the write command.)*
3. **`03-param-change.pcapng`** — change **one** knob by a known amount (e.g.
   a distortion's Drive/Gain from ~50% to ~80%). Note block, param, from→to.
   *(Reveals write + value encoding.)*
4. **`04-param-change-native.pcapng`** — change a param with real units (an EQ
   band in dB, or a delay Time in ms / note value). Note exact from→to.
   *(Reveals native-unit encoding/range.)*
5. **`05-model-change.pcapng`** — swap the model in one FX slot (e.g. one
   distortion → another). Note slot, from-model → to-model.
6. **`06-store.pcapng`** — press Save/Store to write the edit buffer to a preset
   slot. Note the slot number.
7. **`07-load-patch.pcapng`** — select a different preset. (Mostly read; confirms
   the load/select sequence.)
8. *(optional)* `08-add-remove-block`, `09-move-block`, `10-snapshot-change`.

Tip: do each action **slowly and singly** — one knob, one toggle — so the diff
is unambiguous. Avoid moving other controls in the same capture.

## 4. Export for analysis
Easiest for offline analysis: from the Wireshark install, run **tshark** in a
terminal to dump the bulk payloads as hex, one per line:
```
tshark -r 02-bypass-toggle.pcapng ^
  -Y "usb.endpoint_address==0x01 || usb.endpoint_address==0x81" ^
  -T fields -e frame.number -e usb.endpoint_address -e usb.capdata ^
  > 02-bypass-toggle.txt
```
Share the `.txt` files (and/or the `.pcapng`s). If an action showed nothing on
the bulk endpoints, re-export without the `-Y` filter so we can see which
endpoint it used.

Copy the files into the repo (e.g. a `captures/` folder) on the Linux side and
tell me — I'll decode the MessagePack payloads, diff before/after, and work out
the write command structure + value encoding.

## 5. Quick reference — known packet structure
Every packet (see `hx-protocol.md`): bytes 0..3 length/marker, **4..7 channel
magic** (`01 10 EF 03`=x1, `80 10 ED 03`=x80, `02 10 F0 03`=x2), **8..9 seq**,
**10..11 command** (`0x04` open, `0x08` chunk, `0x0C` paged-stream — writes use
an as-yet-unknown command/flag), then payload. Resource opens carry MessagePack
`{102: resource, 100: param, 101: extra}`. Preset data = resource **1012**.
What we're hunting in the write captures: the command/flag that opens a writable
handle, how data chunks are pushed, and the commit step (the read side acks 3
chunks then stalls — the captures will show what comes between/after).
