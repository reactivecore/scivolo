//! Finding the main classes of compiled NIR: the classes with a static `main(Array[String])`.

use std::path::Path;

/// The mangled signature of the static `main(Array[String]): Unit` of a main class.
const MAIN: &str = "D4mainLAL16java.lang.String_uEo";

/// Adds the classes of the NIR files under `dir` that define `main` to `mains`.
pub fn in_dir(dir: &Path, mains: &mut Vec<String>) -> Result<(), String> {
    let entries = std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    for path in entries.flatten().map(|e| e.path()) {
        if path.is_dir() {
            in_dir(&path, mains)?;
        } else if path.extension().is_some_and(|e| e == "nir") {
            let data = std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
            mains.extend(defined(&data).map_err(|e| format!("{}: {e}", path.display()))?);
        }
    }
    Ok(())
}

/// The class in NIR file `data` that defines `main`, if any.
pub fn defined(data: &[u8]) -> nir::Result<Option<String>> {
    let (mut pool, mut caches) = (nir::pool::Pool::default(), nir::Caches::default());
    let index = nir::Reader::new(data, &mut caches, &mut pool)?.index()?;
    Ok(index.iter().find_map(|&(g, _)| match *pool.global(g) {
        nir::Global::Member(top, sig) if pool.str(sig) == MAIN => match *pool.global(top) {
            nir::Global::Top(name) => Some(pool.str(name).to_owned()),
            _ => None,
        },
        _ => None,
    }))
}
