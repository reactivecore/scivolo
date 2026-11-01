// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/tasty/TastyPickler.scala
use super::buffer::{Buffer, pjw_hash64};
use super::names::NameTable;
use crate::file::MAGIC;
use crate::names::NameRef;
use crate::tags::{MAJOR_VERSION, MINOR_VERSION};

/// The tooling version written into the header.
pub const TOOLING: &str = "Scala 3.9.0";

/// A TASTy file in the making: the name table and the sections, the tree section first.
#[derive(Debug, Default)]
pub struct Pickler {
    pub names: NameTable,
    sections: Vec<(NameRef, Vec<u8>)>,
}

impl Pickler {
    /// Registers the name of a section, whose bytes `set_section` gives later.
    pub fn new_section(&mut self, name: &str) -> usize {
        self.sections.push((self.names.utf8(name), Vec::new()));
        self.sections.len() - 1
    }

    pub fn set_section(&mut self, section: usize, bytes: Vec<u8>) {
        self.sections[section].1 = bytes;
    }

    /// The file: header with the UUID hashed from the parts, name table, sections.
    pub fn assemble(&self) -> Vec<u8> {
        let names = self.names.assemble();
        let mut hashes = self.sections.iter().map(|(_, b)| pjw_hash64(b));
        let tree_hash = hashes.next().expect("a TASTy file has a tree section");
        let uuid_low = pjw_hash64(&names.bytes) ^ tree_hash;
        let uuid_high = hashes.fold(0, |h, x| h ^ x);
        let mut all = Buffer::default();
        all.write_bytes(&MAGIC);
        all.write_nat(MAJOR_VERSION);
        all.write_nat(MINOR_VERSION);
        all.write_nat(0);
        all.write_utf8(TOOLING);
        all.write_uncompressed_long(uuid_low);
        all.write_uncompressed_long(uuid_high);
        all.write_nat(names.len() as u64);
        all.write_bytes(&names.bytes);
        for (name, bytes) in &self.sections {
            all.write_nat(name.0 as u64);
            all.write_nat(bytes.len() as u64);
            all.write_bytes(bytes);
        }
        all.bytes
    }
}
