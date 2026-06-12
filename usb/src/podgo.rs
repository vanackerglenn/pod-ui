use std::path::PathBuf;
use std::process::Command;
use log::*;

static PRESET_NAMES: once_cell::sync::OnceCell<Vec<(u16, String)>> = once_cell::sync::OnceCell::new();

pub fn preset_names() -> Option<&'static Vec<(u16, String)>> {
    PRESET_NAMES.get()
}

/// Find a probe binary by name (e.g. "podgo_probe", "podgo_current_probe")
fn find_probe(name: &str) -> Option<PathBuf> {
    if let Ok(exe) = std::env::current_exe() {
        let dir = exe.parent().unwrap();
        let same_dir = dir.join(name);
        if same_dir.exists() {
            return Some(same_dir);
        }
        let examples_dir = dir.join("examples").join(name);
        if examples_dir.exists() {
            return Some(examples_dir);
        }
    }
    let cwd = PathBuf::from(name);
    if cwd.exists() { return Some(cwd); }
    None
}

/// Fetch POD Go preset names by running the podgo_probe binary as a
/// subprocess. The standalone probe works reliably, and shelling out
/// avoids any libusb in-process context conflicts.
pub fn fetch_and_cache() {
    info!("Fetching POD Go preset names via subprocess...");

    let probe_path = match find_probe("podgo_probe") {
        Some(p) => p,
        None => {
            warn!("podgo_probe binary not found");
            return;
        }
    };

    let output = match Command::new(&probe_path).output() {
        Ok(o) => o,
        Err(e) => {
            warn!("Failed to run probe: {}", e);
            return;
        }
    };

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        warn!("Probe failed: {}", stderr.trim());
        return;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut names = Vec::new();

    for line in stdout.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix('[') {
            if let Some(end_bracket) = rest.find(']') {
                let index_str = &rest[..end_bracket].trim();
                let name = rest[end_bracket + 1..].trim().to_string();
                if let Ok(index) = index_str.parse::<u16>() {
                    if !name.is_empty() {
                        names.push((index, name));
                    }
                }
            }
        }
    }

    if names.is_empty() {
        warn!("No preset names parsed from probe output");
        return;
    }

    let _ = PRESET_NAMES.set(names);
    info!("Cached {} preset names", PRESET_NAMES.get().unwrap().len());
}

// ========== Current Preset Data ==========

#[derive(Debug, Clone)]
pub struct ModuleInfo {
    pub name: String,
}

#[derive(Debug, Clone)]
pub struct CurrentPreset {
    pub modules: Vec<ModuleInfo>,
    pub snapshots: Vec<String>,
}

/// Read the current active preset data by running podgo_current_probe as subprocess.
/// Returns module names and snapshot names from the device's current state.
/// Returns None if the probe binary isn't found or fails.
pub fn read_current_preset() -> Option<CurrentPreset> {
    let probe_path = match find_probe("podgo_current_probe") {
        Some(p) => p,
        None => {
            warn!("podgo_current_probe binary not found");
            return None;
        }
    };

    let output = match Command::new(&probe_path).output() {
        Ok(o) => o,
        Err(e) => {
            warn!("Failed to run current probe: {}", e);
            return None;
        }
    };

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        warn!("Current probe failed: {}", stderr.trim());
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut modules = Vec::new();
    let mut snapshots = Vec::new();

    for line in stdout.lines() {
        let line = line.trim();
        if let Some(name) = line.strip_prefix("MODULE: ") {
            modules.push(ModuleInfo { name: name.to_string() });
        } else if let Some(name) = line.strip_prefix("SNAPSHOT: ") {
            snapshots.push(name.to_string());
        }
    }

    if modules.is_empty() {
        warn!("No modules parsed from current probe output");
        return None;
    }

    info!("Read current preset: {} modules, {} snapshots", modules.len(), snapshots.len());
    Some(CurrentPreset { modules, snapshots })
}
