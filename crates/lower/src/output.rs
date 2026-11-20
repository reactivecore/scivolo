//! What the frontend hands on: NIR and TASTy files written under a directory, and panics as
//! messages.

use crate::{Lower, Result};
use std::path::Path;
use typer::pickle::Pickled;
use typer::tpd::TNodeId;

/// A `.nir` file: the top-level name it defines and its bytes.
pub struct NirFile {
    pub name: String,
    pub bytes: Vec<u8>,
}

/// The TASTy of a unit's top-level classes, and where and why the classes of some files were not
/// pickled, if that leaves out their TASTy only.
pub struct Pickles {
    pub classes: Vec<Pickled>,
    pub refused: Vec<String>,
}

/// A `.tasty` file: its path under the output directory, `pkg/Class.tasty`, and its bytes.
pub struct TastyFile {
    pub path: String,
    pub bytes: Vec<u8>,
}

/// The NIR and TASTy files of a unit, and why the classes of some files have no TASTy.
pub struct Lowered {
    pub nir: Vec<NirFile>,
    pub tasty: Vec<TastyFile>,
    pub unpickled: Vec<String>,
}

impl Lowered {
    /// Writes the NIR and TASTy files under `out`, created if missing.
    pub fn write(&self, out: &Path) -> std::result::Result<(), String> {
        let nir = self.nir.iter().map(|f| (format!("{}.nir", f.name.replace('.', "/")), &f.bytes));
        let tasty = self.tasty.iter().map(|f| (f.path.clone(), &f.bytes));
        std::fs::create_dir_all(out).map_err(|e| format!("{}: {e}", out.display()))?;
        for (path, bytes) in nir.chain(tasty) {
            let path = out.join(path);
            let dir = path.parent().unwrap_or(out);
            std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
            std::fs::write(&path, bytes).map_err(|e| format!("{}: {e}", path.display()))?;
        }
        Ok(())
    }
}

/// Generates the NIR files of lowered unit `root` and names the TASTy of `pickles` as dotty
/// writes it beside the class files: by the binary name of the class, an object's without its
/// `$` (`GenBCode`'s `TASTY` file of a top-level class).
pub fn nir_with_tasty(l: &mut Lower, root: TNodeId, pickles: Pickles) -> Result<Lowered> {
    let nir = crate::codegen::unit(l, root)?;
    let tasty = pickles.classes.into_iter().map(|p| {
        let name = l.u.binary_name(p.class);
        let name = name.strip_suffix('$').unwrap_or(&name);
        TastyFile { path: format!("{}.tasty", name.replace('.', "/")), bytes: p.tasty }
    });
    Ok(Lowered { nir, tasty: tasty.collect(), unpickled: pickles.refused })
}

/// Writes `files` under `classes`, created if missing, and returns the main classes among them,
/// sorted.
pub fn write_nir(files: Vec<NirFile>, classes: &Path) -> std::result::Result<Vec<String>, String> {
    let mains = mains(&files)?;
    std::fs::create_dir_all(classes).map_err(|e| format!("{}: {e}", classes.display()))?;
    for f in files {
        let path = classes.join(format!("{}.nir", f.name.replace('.', "/")));
        let dir = path.parent().unwrap_or(classes);
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        std::fs::write(&path, f.bytes).map_err(|e| format!("{}: {e}", path.display()))?;
    }
    Ok(mains)
}

/// Lists the classes among `files` that define `main`, sorted.
pub fn mains(files: &[NirFile]) -> std::result::Result<Vec<String>, String> {
    let mut mains = Vec::new();
    for f in files {
        let main = nir::mains::defined(&f.bytes).map_err(|e| format!("{}: undecodable NIR ({e})", f.name))?;
        mains.extend(main);
    }
    mains.sort();
    Ok(mains)
}

/// The message of `panic`, as an internal error.
pub fn panicked(panic: &(dyn std::any::Any + Send)) -> String {
    let what = panic.downcast_ref::<String>().map(String::as_str).or(panic.downcast_ref::<&str>().copied());
    format!("internal error: {}", what.unwrap_or("panic"))
}
