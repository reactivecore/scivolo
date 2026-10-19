// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/tasty/AttributeUnpickler.scala
//   compiler/src/dotty/tools/dotc/core/tasty/TastyUnpickler.scala
//   tasty/src/dotty/tools/tasty/TastyHeaderUnpickler.scala
use crate::cursor::{Cursor, Result};
use crate::names::{NameRef, Names};
use crate::tags::{MAJOR_VERSION, MINOR_VERSION};

const MAGIC: [u8; 4] = [0x5C, 0xA1, 0xAB, 0x1F];

#[derive(Debug, Clone)]
pub struct Header<'a> {
    pub major: u64,
    pub minor: u64,
    pub experimental: u64,
    pub tooling: &'a str,
    pub uuid: [u8; 16],
}

#[derive(Debug, Clone)]
pub struct Section<'a> {
    pub name: NameRef,
    pub start: usize,
    pub bytes: &'a [u8],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Attribute {
    Flag(u8),
    SourceFile(NameRef),
}

/// A TASTy file with header, name table and section table decoded; sections stay raw.
pub struct Tasty<'a> {
    pub header: Header<'a>,
    pub names: Names<'a>,
    pub sections: Vec<Section<'a>>,
}

impl<'a> Tasty<'a> {
    pub fn parse(bytes: &'a [u8]) -> Result<Self> {
        let mut cur = Cursor::new(bytes);
        let header = read_header(&mut cur)?;
        let end = cur.end()?;
        let names = Names::parse(&mut cur, end)?;
        let mut sections = Vec::new();
        while !cur.at_end() {
            let name = NameRef(cur.nat32()?);
            let end = cur.end()?;
            let start = cur.pos();
            sections.push(Section { name, start, bytes: cur.bytes(end - start)? });
        }
        Ok(Tasty { header, names, sections })
    }

    pub fn section(&self, name: &str) -> Option<&Section<'a>> {
        self.sections
            .iter()
            .find(|s| matches!(self.names.get(s.name), Some(crate::Name::Utf8(n)) if *n == name))
    }

    pub fn attributes(&self) -> Result<Vec<Attribute>> {
        let Some(sec) = self.section("Attributes") else { return Ok(Vec::new()) };
        let mut cur = Cursor::new(sec.bytes);
        let mut out = Vec::new();
        while !cur.at_end() {
            let tag = cur.byte()?;
            out.push(match tag {
                1..=32 => Attribute::Flag(tag),
                129..=160 => Attribute::SourceFile(NameRef(cur.nat32()?)),
                _ => return cur.err(format!("unknown attribute tag {tag}")),
            });
        }
        Ok(out)
    }
}

pub fn attribute_name(tag: u8) -> &'static str {
    match tag {
        1 => "SCALA2STANDARDLIBRARY",
        2 => "EXPLICITNULLS",
        3 => "CAPTURECHECKED",
        4 => "WITHPUREFUNS",
        5 => "JAVA",
        6 => "OUTLINE",
        _ => "?",
    }
}

fn read_header<'a>(cur: &mut Cursor<'a>) -> Result<Header<'a>> {
    if cur.bytes(4)? != MAGIC {
        return cur.err("not a TASTy file");
    }
    let major = cur.nat()?;
    if major != MAJOR_VERSION {
        return cur.err(format!("unsupported TASTy major version {major}"));
    }
    let minor = cur.nat()?;
    let experimental = cur.nat()?;
    if minor > MINOR_VERSION || (minor == MINOR_VERSION && experimental != 0) {
        return cur.err(format!("unsupported TASTy version {major}.{minor}-{experimental}"));
    }
    let tooling = cur.utf8()?;
    let uuid = cur.bytes(16)?.try_into().expect("16 bytes");
    Ok(Header { major, minor, experimental, tooling, uuid })
}

impl Header<'_> {
    pub fn uuid_string(&self) -> String {
        let hex: String = self.uuid.iter().map(|b| format!("{b:02x}")).collect();
        format!("{}-{}-{}-{}-{}", &hex[..8], &hex[8..12], &hex[12..16], &hex[16..20], &hex[20..])
    }
}
