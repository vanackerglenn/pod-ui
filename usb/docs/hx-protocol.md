# HX USB Bulk Protocol — Line 6 HX/Helix/Pod Go

This documents the USB bulk transfer protocol used by Line 6 HX-series devices (Helix, HX Stomp, **Pod Go**), reverse-engineered from live USB captures and hardware testing.

## Sources

- [openhx](https://github.com/allansomensi/openhx) — Rust, HX Stomp XL validated
- [helix_usb](https://github.com/kempline/helix_usb) — Python, HX Stomp validated
- [Helix-usb-RE](https://github.com/flowernert/Helix-usb-RE) — protocol RE notes

## Device Identification

| Device | VID | PID |
|--------|-----|-----|
| Pod Go | `0x0E41` | `0x4247` |
| HX Stomp | `0x0E41` | `0x4246` |
| HX Stomp XL | `0x0E41` | `0x4253` |

Interface 0 (vendor-specific), EP 0x01 (OUT), EP 0x81 (IN).
MIDI on interface 4, EP 0x02 (OUT), EP 0x82 (IN).

## Communication Channels

Three logical channels exist over the same bulk endpoints (0x01/0x81):

| Channel ID | Magic bytes (bytes 4-7) | Purpose |
|---|---|---|
| x1 | `01 10 EF 03` | Session handshake, preset names, keep-alive |
| x2 | `02 10 F0 03` | Keep-alive, signal routing |
| x80 | `80 10 ED 03` | Preset data, device config, keep-alive |

All three channels must be established before the device will accept preset data requests.

## Packet Structure

```
Byte 0:     total length (including this byte)
Bytes 1-3:  fixed (00 00 18 or 00 00 28 for handshake)
Bytes 4-7:  channel magic (01 10 EF 03 / 02 10 F0 03 / 80 10 ED 03)
Bytes 8-9:  sequence number (big-endian), auto-increment per channel
Bytes 10-11: command type
  - 0x0002: handshake/validate
  - 0x0004: open resource / data response
  - 0x0008: read chunk / ACK / keep-alive
  - 0x000C: start paged stream
Bytes 12+:  payload (varies by command)
```

Extended packets (resource operations):
```
Bytes 12-15: object/stream ID (handle)
Bytes 16-17: always 0x0001 (LE)
Bytes 18-19: always 0x0002 (LE)
Bytes 20-23: payload length in bytes (LE)
Bytes 24+:   MessagePack-encoded resource spec
```

## Full Connect Sequence (3 channels)

Required before requesting preset data. The **host initiates all channels** — the device only responds.

### Phase 1: x1 Session (enhanced, sub_type=5)

```
1. HOST → x1 HANDSHAKE         20B: 0C 00 00 28 01 10 EF 03 00 00 00 02 00 01 00 21 00 10 00 00
2. DEV → 20B ACK (echoes magic)
3. HOST → x1 SESSION_OPEN_1    28B: 11 00 00 18 01 10 EF 03 00 02 00 04 00 10 00 00 01 00 05 00 01 00 00 00 05 00 00 00
4. DEV → 68B response (contains "P34Main" + session metadata)
5. HOST → x1 CHUNK_READ        16B: 08 00 00 18 01 10 EF 03 00 03 00 08 20 10 00 00
6. DEV → 16B ack
7. HOST → x1 CMD_0002          16B: 08 00 00 18 01 10 EF 03 00 04 00 02 20 10 00 00
8. DEV → 16B ack (seq=4, cmd=0x0002, status=09 02)
```

Notes:
- Sub_type=5 (vs sub_type=2 in simple handshake) enables multi-channel support
- CMD_0002 **must** use seq=4 (not copied from previous response seq=3)
- SESSION_OPEN_1 response body contains "P34Main" identifying the device model

### Phase 2: x80 Channel (HOST initiated)

After CMD_0002, HOST sends x80 handshake directly:

```
9. HOST → x80 HANDSHAKE        20B: 0C 00 00 28 80 10 ED 03 00 00 00 02 00 01 00 21 00 10 00 00
10. DEV → response (echoes x80 magic)
11. HOST → x80 SESSION_OPEN_1  28B: 11 00 00 18 80 10 ED 03 00 02 00 04 00 10 00 00 01 00 06 00 01 00 00 00 06 00 00 00
12. DEV → response
```

Sub_type=6 is used for x80 sessions.

### Phase 3: x2 Channel (HOST initiated)

```
13. HOST → x2 HANDSHAKE        20B: 0C 00 00 28 02 10 F0 03 00 00 00 02 00 01 00 21 00 10 00 00
14. DEV → response (echoes x2 magic)
15. HOST → x2 SESSION_OPEN_1   28B: 11 00 00 18 02 10 F0 03 00 02 00 04 00 10 00 00 01 00 04 00 01 00 00 00 04 00 00 00
16. DEV → response
```

Sub_type=4 is used for x2 sessions.

### Phase 4: Open resource 1000 on x80

```
17. HOST → x80 OPEN_RSC_1000   36B:
    19 00 00 18 80 10 ED 03 00 03 00 04
    09 10 00 00        # handle
    01 00 06 00 09 00 00 00
    83 66 CD 03 E8 64 4C 65 80 00 00 00   # {102:1000, 100:76, 101:128}
18. DEV → response
```

## Session Handshake (Simple, x1 only — for preset NAMES)

Used when only preset names are needed (no preset data):

1. **HANDSHAKE** (20 bytes) — seq=0
2. **SESSION_OPEN_1** (28 bytes) — sub_type=2, seq=2
3. **CHUNK_READ** (16 bytes) — offset=0x1009, seq=3
4. **SESSION_OPEN_2** (36 bytes) — resource 1000, param=254, seq=4
5. **CHUNK_READ** (16 bytes) — offset=0x101A, seq=5

No CMD_0002 needed. Sequence continues from seq=6.

## Preset Name Listing (x1 channel)

After 5-packet handshake on x1:

1. **OPEN_PRESETS** (36 bytes) — `{102:1001, 100:0, 101:nil}`, cmd=0x0004, seq=6
2. **OPEN_STREAM** (40 bytes) — `{102:1002, 100:1, 101:{107:0, 101:2}}`, cmd=0x000C, seq=7 → first data chunk
3. **CHUNK_LOOP** — request subsequent chunks (seq=8+, offset=0x1138, inc 0x100) until short read

Response is MessagePack: preamble bytes + `fixmap(3) {102:1002, 103:0, 104: array16(128) [...]}`

Each preset in the array: `fixmap(1) {index: fixmap(4) {109:"name\0", 123:bool, 124:bool, 125:int}}`

Field keys in preset data:
- 109 (`m`): preset name (null-terminated string)
- 123 (`{`): boolean (favorite?)
- 124 (`|`): boolean (edited?)
- 125 (`}`): integer

## Preset Data Read (x80 channel)

After full 3-channel connect:

1. **REQUEST_PRESET** (36 bytes) — `{102:1012, 100:22, 101:nil}`, cmd=0x000C:
   ```
   19 00 00 18 80 10 ED 03 00 <seq> 00 0C
   <session_no> <cnt0> <cnt1> 00   # handle/counter
   01 00 06 00 09 00 00 00
   83 66 CD 03 F4 64 16 65 C0 00 00 00   # {102:0x3F4=1012, 100:0x16=22, 101:nil}
   ```

2. Device responds with first data chunk (272 bytes: 16 header + 256 payload).

3. Read subsequent chunks by sending x80 keep-alive/dummy packets. Each write+read returns the next chunk.

4. Last chunk may be shorter than 272 bytes. After a short read (< 20 bytes), data is complete.

Typical transfer: ~16 chunks, ~4KB total.

## Binary Preset Data Format

The accumulated preset data (payload bytes 16+ from each chunk concatenated) encodes the current device state. It is NOT MessagePack — it uses a custom binary format with section delimiters:

### Section Delimiters (hex string markers)

| Marker | Purpose |
|--------|---------|
| `8213` | Section separator within slot data |
| `8215` | Slot section boundary |
| `9187` | **Module entry** (signal block start) |
| `0895` | Separates main data from footswitch data |

### Module Entry Format

Each module in the signal chain starts with `91 87` followed by:

```
91 87                       # module entry marker
0a 00 0b 85 00 01          # unknown header
05                          # key 5 (module name)
<fixstr> <name_bytes>\0     # MessagePack fixstr with null-terminated name
06                          # key 6 (parameter/ID)
<value>                     # parameter value (varies: uint32/uint16/float)
07                          # key 7 (bypass state)
<c2|c3>                     # false=true, true=bypassed
08                          # key 8 (slot index)
<fixint>                    # slot position in chain
0c                          # key 12
<c2|value>                 # additional state
0e a1 00                   # routing target info
0d                          # key 13
c2                          # false
10 00 0f                   # unknown
c2                          # false (end of module)
```

Module data may be preceded by routing/mixer sections (starting with patterns like `1a ff 09...`) that define the signal flow between blocks.

### Snapshot Markers

Snapshots appear after module data, formatted as:
```
91 87 0a 00 0b 85 00 01 05 <fixstr>"SNAPSHOT N"\0 06 ...
```

Followed by per-snapshot parameter override values.

### Footswitch Data (after `0895` marker)

After the `0895` separator byte, footswitch assignments and LED state data follows.

### String Values Commonly Found

- Firmware ID: "P34" (Pod Go), "l6-helix"
- Firmware version: "v2.00-5-g665e64e"
- Module names: see [Module Names](#module-names)
- Snapshot names: "SNAPSHOT 1" through "SNAPSHOT 4"

## Requesting Preset NAMES vs DATA

| Operation | Channel | Resource | Param | Cmd |
|-----------|---------|----------|-------|-----|
| List all 128 preset names | x1 | 1001 | 0 | 0x0004 + 0x000C |
| Request current preset data | x80 | 1012 | 22 (0x16) | 0x000C |

The name listing (resource 1001 on x1) uses a simple 5-packet handshake.
The data request (resource 1012 on x80) requires the full 3-channel connect.

## Channels & Keep-Alive

After the main connect sequence establishes all 3 channels, keep-alive packets must be sent at ~1Hz on each channel to prevent timeout:

- **x1 keep-alive**: `[0x8,0,0,0x18, 1,0x10,0xEF,3, 0,seq, 0,8, 0x72,0x1E,0,0]`
- **x2 keep-alive**: `[0x8,0,0,0x18, 2,0x10,0xF0,3, 0,seq, 0,0x10, 9,0x10,0,0]`
- **x80 keep-alive**: `[0x8,0,0,0x18, 0x80,0x10,0xED,3, 0,seq, 0,0x10, session_no,cnt0,cnt1,0]`

Without keep-alive, the device stops responding after ~1-2s.

Data chunks on x80 are returned in response to any valid x80 write (including keep-alive packets).

## Verified Resources

| Resource | Channel | Works | Content |
|----------|---------|-------|---------|
| 1000 (0x3E8) | x80 | ✓ | Device bundle/session |
| 1001 (0x3E9) | x1 | ✓ | All 128 preset names |
| 1002 (0x3EA) | x1 | ✓ | Stream trigger for names |
| 1003-1006 | x1 | ✗ | NAK (not available on x1) |
| 1012 (0x3F4) | x80 | ✓ | Current preset data |
| 1013 (0x3F5) | x80 | ✗ | Not tested |
