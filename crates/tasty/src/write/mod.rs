//! Encodes TASTy files: buffers, the name table, the tree section's references, the positions
//! and the header.
mod buffer;
mod name;
mod names;
mod pickler;
mod positions;
mod tree_buffer;

pub use buffer::{ADDR_WIDTH, Addr, Buffer, nat_size, pjw_hash64};
pub use name::{Name, ParamSig};
pub use names::NameTable;
pub use pickler::{Pickler, TOOLING};
pub use positions::{PositionsBuffer, text_line_sizes};
pub use tree_buffer::TreeBuffer;
