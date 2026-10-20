//! Finding the main classes of compiled NIR.

use std::path::Path;

/// Adds the classes of the NIR files under `dir` that define `main` to `mains`.
pub fn in_dir(dir: &Path, mains: &mut Vec<String>) -> Result<(), String> {
    let entries = std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    for path in entries.flatten().map(|e| e.path()) {
        if path.is_dir() {
            in_dir(&path, mains)?;
        } else if path.extension().is_some_and(|e| e == "nir") {
            let data = std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
            mains.extend(nir::mains::defined(&data).map_err(|e| format!("{}: {e}", path.display()))?);
        }
    }
    Ok(())
}
