//! Locates the files of an installed distribution: `bin/` holds the programs, `lib/` what they
//! read at run time.

use std::path::PathBuf;

/// Determines the `lib/` directory of the distribution the running program belongs to:
/// `<dir of the program, symlinks resolved>/../lib`, if it exists. A program built in the
/// workspace has none and reads its files from the source and build tree.
pub fn lib_dir() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?.canonicalize().ok()?;
    let lib = exe.parent()?.parent()?.join("lib");
    lib.is_dir().then_some(lib)
}

/// Determines where file `rel` lives: under the distribution's `lib/` if the program runs from
/// a distribution, else at `dev`, its place in the source or build tree.
pub fn file(rel: &str, dev: impl Into<PathBuf>) -> PathBuf {
    match lib_dir() {
        Some(lib) => lib.join(rel),
        None => dev.into(),
    }
}
