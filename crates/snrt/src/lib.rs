//! Scala Native's C runtime (posixlib, clib, javalib and parts of nativelib), built as a shared
//! library for the VM to load and as a static archive for native executables.

/// Where the build placed the shared library.
pub const PATH: &str = env!("SNRT_PATH");

/// Where the build placed the static archive.
pub const ARCHIVE: &str = env!("SNRT_ARCHIVE");
