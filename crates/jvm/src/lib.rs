//! A JVM backend: class files from the lowered trees (docs/research/46-jvm-backend.md).

mod codegen;
pub mod jar;

pub use codegen::{ClassBytes, unit};
