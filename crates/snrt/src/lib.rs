//! Scala Native's C runtime (posixlib, clib, javalib and parts of nativelib), built as a shared
//! library for the VM to load.

/// Where the build placed the shared library.
pub const PATH: &str = env!("SNRT_PATH");
