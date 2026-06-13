// Offline parser check: decode a captured Pod Go preset .bin and print modules.
// Usage: cargo run -p pod-usb --example podgo_parse_check -- /tmp/podgo_preset.bin
fn main() {
    let path = std::env::args().nth(1).unwrap_or_else(|| "/tmp/podgo_preset.bin".to_string());
    let data = std::fs::read(&path).expect("read preset bin");
    let preset = pod_usb::parse_preset_data(&data);
    println!("Parsed {} modules from {} ({} bytes):", preset.modules.len(), path, data.len());

    // Mirror the handler's routing: fixed blocks go to dedicated controls,
    // everything else fills the next free FX slot (1..=4), in chain-slot order.
    let mut modules: Vec<&pod_usb::ModuleInfo> = preset.modules.iter().collect();
    modules.sort_by_key(|m| m.slot);

    let mut next_fx_slot = 0usize;
    for m in &modules {
        let routing = if pod_usb::is_fixed_block_category(&m.category, &m.name) {
            "fixed".to_string()
        } else if next_fx_slot < 4 {
            next_fx_slot += 1;
            format!("FX slot {}", next_fx_slot)
        } else {
            "OVERFLOW (>4 FX, not shown)".to_string()
        };
        println!("  slot {:>2}  [{:<18}] {:<20} bypassed={:<5} -> {:<14} params={:?}",
            m.slot, m.category, m.name, m.bypassed, routing, m.parameters);
    }
}
