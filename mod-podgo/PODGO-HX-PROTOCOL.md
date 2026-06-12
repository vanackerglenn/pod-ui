POD Go / HX Series USB Bulk Protocol
=====================================
Date: 2026-06-11
Source: OpenHX reverse-engineering project (https://github.com/allansomensi/openhx)
Device: Line6 POD Go (VID 0x0E41, PID 0x4247)
References: HX Stomp XL (VID 0x0E41, PID 0x4253) — shares the same protocol

=====================================================================
1. Overview
=====================================================================

Pod Go (and all HX/Helix series devices) communicate over USB BULK
transfers on the vendor-specific interface. This replaces the older
MIDI/SysEx approach used by POD 2.0 / XT family devices.

The protocol:
- Uses USB Bulk transfers (not MIDI)
- Is strictly request/response (write, then read)
- Is session-based (5-packet handshake to initialize)
- Encodes preset data as MessagePack (not nibble-encoded SysEx)

=====================================================================
2. USB Interface Layout
=====================================================================

Interface 0 (Vendor Specific, class 0xFF):
  EP 0x01 OUT (Bulk, maxPacket=512) — Host -> Device
  EP 0x81 IN  (Bulk, maxPacket=512) — Device -> Host

All protocol communication happens on this interface.
Interface 4 (USB-MIDI) is used only for real-time PC/CC messages.

=====================================================================
3. Packet Format
=====================================================================

All packets share a common header format:

Byte(s)  Description
-------  -----------
[0-1]    Payload length (LE u16)
[2]      Flags (usually 0x00)
[3]      Protocol version (0x18)
[4-5]    Source endpoint (0x01 0x10 = 0x1001 = primary channel)
[6-7]    Destination endpoint (0xEF 0x03 = 0x03EF = device)
[8]      Unknown (usually 0x00)
[9]      Sequence number (increments per packet, wraps 0xFF->0x00)
[10]     Unknown (usually 0x00)
[11]     Command byte
[12..15] Command-specific data (offset, etc.)
[16..n]  Additional payload (varies by command)

=====================================================================
4. Session Initialization (5-Packet Handshake)
=====================================================================

Before any application command, send these 5 packets in order.
After each, perform a bulk IN read (response is discarded).

Step 1 — HANDSHAKE (20 bytes):
  0C 00 00 28 01 10 EF 03 00 00 00 02 00 01 00 21
  00 10 00 00

Step 2 — SESSION_OPEN_1 (28 bytes, seq=0x02, cmd=0x04):
  11 00 00 18 01 10 EF 03 00 02 00 04 00 10 00 00
  01 00 02 00 01 00 00 00 02 00 00 00

Step 3 — SESSION_CHUNK_1 (16 bytes, seq=0x03, cmd=0x08):
  08 00 00 18 01 10 EF 03 00 03 00 08 09 10 00 00

Step 4 — SESSION_OPEN_2 (36 bytes, seq=0x04, cmd=0x04):
  1A 00 00 18 01 10 EF 03 00 04 00 04 09 10 00 00
  01 00 02 00 0A 00 00 00 83 66 CD 03 E8 64 CC FE
  65 80 00 00

Step 5 — SESSION_CHUNK_2 (16 bytes, seq=0x05, cmd=0x08):
  08 00 00 18 01 10 EF 03 00 05 00 08 1A 10 00 00

After step 5, seq=0x06 is available for application commands.

=====================================================================
5. Preset Listing Protocol
=====================================================================

Phase 1 — Open Preset Resource (seq=0x06, cmd=0x04, 36 bytes):
  19 00 00 18 01 10 EF 03 00 06 00 04 1A 10 00 00
  01 00 02 00 09 00 00 00 83 66 CD 03 E9 64 00 65
  C0 00 00 00
  -> write, then read+discard

Phase 2 — Start Paged Stream (seq=0x07, cmd=0x0C, 40 bytes):
  1D 00 00 18 01 10 EF 03 00 07 00 0C 38 10 00 00
  01 00 02 00 0D 00 00 00 83 66 CD 03 EA 64 01 65
  82 6B 00 65 02 00 00 00
  -> write, then read. Response[16..n] = first chunk data.

Phase 3 — Paged Chunk Loop:
  For each subsequent chunk:
    seq = 0x08, 0x09, ... (incrementing)
    offset = 0x00001138, then +0x0100 per chunk
    Request (16 bytes):
      [0x08, 0x00, 0x00, 0x18, 0x01, 0x10, 0xEF, 0x03,
       0x00, seq,  0x00, 0x08, offset[0..3] (LE)]
    -> write, then read. Append response[16..n] to buffer.

Phase 4 — End of Stream:
  When read returns n < 272 bytes, the stream is exhausted.
  Reassemble all chunks, find DC 00 80 (MessagePack array16 header),
  parse the 128-element preset array.

=====================================================================
6. Preset Data Format (MessagePack)
=====================================================================

The reassembled stream is MessagePack-encoded. Locate the marker:

  DC 00 80   = array16 with 128 elements

Each element is a fixmap(1) with:
  key:   u16 (preset index, 0-127)
  value: map containing preset fields

Known field keys:
  key 109 -> str (preset name, null-terminated)

Extraction:
  for each element in root array:
    outer_map = fixmap(1)
    index     = outer_map[0].key     // u16 preset index
    inner_map = outer_map[0].value   // map
    raw_name  = inner_map[key=109]   // string
    name      = raw_name.trim_end_matches('\0')

Note: MessagePack string lengths include the null terminator.
Always strip trailing null bytes.

=====================================================================
7. Implementation Checklist
=====================================================================

[ ] Open device by VID 0x0E41 / PID 0x4247
[ ] Set USB configuration 1
[ ] Claim interface 0
[ ] Clear halt on endpoints 0x01 and 0x81
[ ] Drain stale IN data (read with 50ms timeout until timeout)
[ ] Send 5 init packets (handshake + 2x open + 2x chunk)
[ ] Send OPEN_PRESETS (seq=0x06), discard response
[ ] Send OPEN_STREAM (seq=0x07), collect response[16..n]
[ ] Loop chunk requests (seq from 0x08, offset from 0x1138)
[ ] Stop when read returns n < 272
[ ] Locate DC 00 80 in reassembled buffer
[ ] Parse MessagePack array of 128 presets
[ ] Strip trailing null bytes from names

=====================================================================
8. Key Differences from Older POD Protocols
=====================================================================

                    POD 2.0 / Pocket POD        HX / POD Go
                   -----------------------     --------------------
Transport          MIDI SysEx over USB-MIDI    USB Bulk on vendor iface
Data encoding      Nibble-encoded              MessagePack
Patch dump format  F0 00 01 0C 01 01 ... F7   Bulk stream with 16B header
Session init       None required               5-packet handshake
Interface          4 (USB Audio/MIDI)          0 (Vendor Specific)
Endpoints          0x82/0x02 (Bulk)            0x81/0x01 (Bulk)
Preset count       124                         128
Endpoint topology  Single pair                 3 interfaces claimed

=====================================================================
9. References
=====================================================================

- OpenHX project: https://github.com/allansomensi/openhx
- OpenHX protocol docs: https://github.com/allansomensi/openhx/tree/main/docs/protocol
- USB analysis: PODGO-USB-ANALYSIS.txt (this directory)
