//! What the frontend hands on: NIR files written under a directory, and panics as messages.

use crate::NirFile;
use std::path::Path;

/// Writes `files` under `classes`, created if missing, and returns the main classes among them,
/// sorted.
pub fn write_nir(files: Vec<NirFile>, classes: &Path) -> Result<Vec<String>, String> {
    std::fs::create_dir_all(classes).map_err(|e| format!("{}: {e}", classes.display()))?;
    let mut mains = Vec::new();
    for f in files {
        let path = classes.join(format!("{}.nir", f.name.replace('.', "/")));
        let dir = path.parent().unwrap_or(classes);
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        let main = nir::mains::defined(&f.bytes).map_err(|e| format!("{}: undecodable NIR ({e})", f.name))?;
        mains.extend(main);
        std::fs::write(&path, f.bytes).map_err(|e| format!("{}: {e}", path.display()))?;
    }
    mains.sort();
    Ok(mains)
}

/// The message of `panic`, as an internal error.
pub fn panicked(panic: &(dyn std::any::Any + Send)) -> String {
    let what = panic.downcast_ref::<String>().map(String::as_str).or(panic.downcast_ref::<&str>().copied());
    format!("internal error: {}", what.unwrap_or("panic"))
}
