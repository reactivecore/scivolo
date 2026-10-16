//! Reader and writer for Scala Native 0.5 NIR (compat 6, revision 11).

pub mod cursor;
pub mod decode;
pub mod demangle;
pub mod encode;
pub mod mains;
pub mod nir;
mod operands;
pub mod pool;
pub mod prelude;
pub mod show;
pub mod sig;
mod successors;
mod tags;

pub use cursor::{Error, Result};
pub use decode::{Caches, Reader};
pub use nir::*;
pub use pool::Pool;
