//! Scala Native's C runtime (posixlib, clib, javalib and parts of nativelib), built as a shared
//! library for the VM to load and as a static archive for native executables.

use std::path::{Path, PathBuf};

/// Where the build placed the shared library.
const PATH: &str = env!("SNRT_PATH");

/// Where the build placed the static archive.
const ARCHIVE: &str = env!("SNRT_ARCHIVE");

/// Locates the shared library: `lib/` of the distribution, else where the build placed it.
pub fn shared_library() -> PathBuf {
    dist::file(&file_name(PATH), PATH)
}

/// Locates the static archive: `lib/aot/` of the distribution, else where the build placed it.
pub fn archive() -> PathBuf {
    dist::file(&format!("aot/{}", file_name(ARCHIVE)), ARCHIVE)
}

fn file_name(path: &str) -> String {
    Path::new(path).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}
