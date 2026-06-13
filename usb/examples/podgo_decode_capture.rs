// Decode POD Go USB captures (from Wireshark/USBPcap) to reverse the protocol.
//
// Input is a tshark field export of the vendor bulk endpoints, produced per the
// capture guide (usb/docs/podgo-usb-capture-guide.md):
//
//   tshark -r capture.pcapng \
//     -Y "usb.endpoint_address==0x01 || usb.endpoint_address==0x81" \
//     -T fields -e frame.number -e usb.endpoint_address -e usb.capdata \
//     > capture.txt
//
// Subcommands:
//   packets <capture.txt>           decode each packet: dir, channel, seq, cmd,
//                                   status, payload hex, and MessagePack if any.
//   stream  <capture.txt> [out.bin] reassemble the device's inbound data stream
//                                   into a preset .bin, then parse + print it.
//   diff    <a.bin> <b.bin>         diff two preset .bins (e.g. before/after a
//                                   param change) — shows which value changed,
//                                   revealing the encoding.
//
// Usage: cargo run -p pod-usb --example podgo_decode_capture -- <subcommand> ...

use std::fs;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let usage = "usage: podgo_decode_capture <packets|stream|diff> <file> [file2]";
    match args.get(1).map(|s| s.as_str()) {
        Some("packets") => cmd_packets(args.get(2).expect(usage)),
        Some("stream")  => cmd_stream(args.get(2).expect(usage), args.get(3).map(|s| s.as_str())),
        Some("diff")    => cmd_diff(args.get(2).expect(usage), args.get(3).expect(usage)),
        _ => { eprintln!("{usage}"); std::process::exit(2); }
    }
}

/// One parsed tshark line: frame number, endpoint address, payload bytes.
struct Pkt { frame: String, ep: u8, data: Vec<u8> }

fn parse_tshark(path: &str) -> Vec<Pkt> {
    let text = fs::read_to_string(path).expect("read capture txt");
    let mut out = vec![];
    for line in text.lines() {
        let f: Vec<&str> = line.split('\t').collect();
        if f.len() < 3 || f[2].trim().is_empty() { continue; }
        let ep = parse_u8(f[1]);
        let hex: String = f[2].chars().filter(|c| c.is_ascii_hexdigit()).collect();
        let data: Vec<u8> = (0..hex.len()/2)
            .map(|i| u8::from_str_radix(&hex[i*2..i*2+2], 16).unwrap_or(0)).collect();
        out.push(Pkt { frame: f[0].to_string(), ep, data });
    }
    out
}

fn parse_u8(s: &str) -> u8 {
    let s = s.trim();
    if let Some(h) = s.strip_prefix("0x") { u8::from_str_radix(h, 16).unwrap_or(0) }
    else { s.parse().or_else(|_| u8::from_str_radix(s, 16)).unwrap_or(0) }
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect::<Vec<_>>().join(" ")
}

/// Channel from the magic at bytes 4..8 (request and swapped-response forms).
fn channel(b: &[u8]) -> &'static str {
    match b {
        [0x01,0x10,0xEF,0x03] | [0xEF,0x03,0x01,0x10] => "x1 ",
        [0x80,0x10,0xED,0x03] | [0xED,0x03,0x80,0x10] => "x80",
        [0x02,0x10,0xF0,0x03] | [0xF0,0x03,0x02,0x10] => "x2 ",
        _ => "???",
    }
}

/// Try to decode a MessagePack value at the start of `b`; return a compact debug
/// string if it parses to something non-trivial.
fn try_msgpack(b: &[u8]) -> Option<String> {
    if b.len() < 2 { return None; }
    let mut cur = b;
    match rmpv::decode::read_value(&mut cur) {
        Ok(v) if !matches!(v, rmpv::Value::Nil) => {
            let s = format!("{v:?}");
            Some(if s.len() > 200 { format!("{}…", &s[..200]) } else { s })
        }
        _ => None,
    }
}

fn cmd_packets(path: &str) {
    println!("frame  dir chan seq  cmd    status  payload");
    for p in parse_tshark(path) {
        let dir = match p.ep { 0x01 => "OUT", 0x81 => "IN ", _ => "?? " };
        let b = &p.data;
        if b.len() < 12 {
            println!("{:>5}  {dir} short ({}B) {}", p.frame, b.len(), hex(b));
            continue;
        }
        let chan = channel(&b[4..8]);
        let seq = b[9];
        let cmd = ((b[10] as u16) << 8) | b[11] as u16;
        let status = if b.len() >= 14 { format!("{:02x}{:02x}", b[12], b[13]) } else { "----".into() };
        // payload after the 12-byte header; try MessagePack at the extended
        // offset (24) first, then right after the header (12).
        let mp = b.get(24..).and_then(try_msgpack)
            .or_else(|| b.get(12..).and_then(try_msgpack));
        let tail = &b[12..b.len().min(12 + 28)];
        println!("{:>5}  {dir} {chan} {seq:02x}   0x{cmd:04x} {status}  {}{}",
            p.frame, hex(tail),
            mp.map(|m| format!("   msgpack={m}")).unwrap_or_default());
    }
}

fn cmd_stream(path: &str, out: Option<&str>) {
    // Reassemble the device's inbound data: each IN chunk carries a 16-byte
    // header followed by stream bytes (matches current_preset.rs).
    let mut all = vec![];
    for p in parse_tshark(path) {
        if p.ep == 0x81 && p.data.len() > 16 {
            all.extend_from_slice(&p.data[16..]);
        }
    }
    let out = out.unwrap_or("/tmp/podgo_capture_preset.bin");
    fs::write(out, &all).expect("write bin");
    println!("reassembled {} inbound bytes -> {out}", all.len());
    let preset = pod_usb::parse_preset_data(&all);
    println!("parsed {} modules:", preset.modules.len());
    for m in &preset.modules {
        println!("  slot {:>2} [{:<14}] {:<20} bypassed={:<5} params={:?}",
            m.slot, m.category, m.name, m.bypassed, m.parameters);
    }
}

fn cmd_diff(a: &str, b: &str) {
    let pa = pod_usb::parse_preset_data(&fs::read(a).expect("read a"));
    let pb = pod_usb::parse_preset_data(&fs::read(b).expect("read b"));
    println!("diff {a} -> {b}");
    let mut any = false;
    for ma in &pa.modules {
        let Some(mb) = pb.modules.iter().find(|m| m.slot == ma.slot) else {
            println!("  slot {:>2}: removed ({})", ma.slot, ma.name); any = true; continue;
        };
        if ma.name != mb.name {
            println!("  slot {:>2}: model {} -> {}", ma.slot, ma.name, mb.name); any = true;
        }
        if ma.bypassed != mb.bypassed {
            println!("  slot {:>2} {}: bypassed {} -> {}", ma.slot, mb.name, ma.bypassed, mb.bypassed); any = true;
        }
        for (i, (va, vb)) in ma.parameters.iter().zip(mb.parameters.iter()).enumerate() {
            if format!("{va:?}") != format!("{vb:?}") {
                println!("  slot {:>2} {}: param[{i}] {va:?} -> {vb:?}", ma.slot, mb.name, ); any = true;
            }
        }
        if ma.parameters.len() != mb.parameters.len() {
            println!("  slot {:>2} {}: param count {} -> {}", ma.slot, mb.name, ma.parameters.len(), mb.parameters.len()); any = true;
        }
    }
    for mb in &pb.modules {
        if !pa.modules.iter().any(|m| m.slot == mb.slot) {
            println!("  slot {:>2}: added ({})", mb.slot, mb.name); any = true;
        }
    }
    if !any { println!("  (no differences in parsed modules/params)"); }
}
