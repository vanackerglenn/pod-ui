use log::*;

static PRESET_NAMES: once_cell::sync::OnceCell<Vec<(u16, String)>> = once_cell::sync::OnceCell::new();

pub fn preset_names() -> Option<&'static Vec<(u16, String)>> {
    PRESET_NAMES.get()
}

/// Fetch POD Go preset names in-process (via `protocol::fetch_preset_names`) and
/// cache them. Replaces the previous subprocess shell-out to the `podgo_probe`
/// example binary.
pub fn fetch_and_cache() {
    info!("Fetching POD Go preset names...");

    let names = match crate::protocol::fetch_preset_names() {
        Ok(names) => names,
        Err(e) => {
            warn!("Failed to read preset names: {}", e);
            return;
        }
    };

    if names.is_empty() {
        warn!("No preset names parsed from device");
        return;
    }

    let count = names.len();
    let _ = PRESET_NAMES.set(names);
    info!("Cached {} preset names", count);
}
