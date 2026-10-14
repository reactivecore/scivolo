//! A loaded top-level file: its bytes, and its trees and names decoded on first use.

use super::bodies::BodyInfo;
use crate::{Ast, NodeId, Tasty};
use std::borrow::Cow;
use std::cell::OnceCell;

/// A file's trees, and its name table shown as text.
pub struct Decoded {
    pub ast: Ast,
    pub names: Vec<String>,
    /// Whether each name is of a kind other than `Utf8`.
    pub derived: Vec<bool>,
    /// Whether the file is of the Scala 2 standard library (`SCALA2STANDARDLIBRARY`).
    pub scala2: bool,
}

impl Decoded {
    pub fn parse(bytes: &[u8]) -> crate::Result<Decoded> {
        let tasty = Tasty::parse(bytes)?;
        let Some(section) = tasty.section("ASTs") else {
            return Err(crate::Error { pos: 0, msg: "no ASTs section".into() });
        };
        let ast = Ast::parse(section.bytes)?;
        let attributes = tasty.attributes()?;
        Ok(Decoded {
            ast,
            names: tasty.names.iter().map(|(r, _)| tasty.names.show(r)).collect(),
            derived: tasty.names.iter().map(|(_, n)| !matches!(n, crate::Name::Utf8(_))).collect(),
            scala2: attributes.iter().any(|a| matches!(a, crate::Attribute::Flag(1))),
        })
    }
}

pub struct Loaded {
    /// Its path in the library.
    pub path: String,
    pub bytes: Cow<'static, [u8]>,
    pub scala2: bool,
    decoded: OnceCell<Decoded>,
    /// The trees its symbol table entered and their tags, checked when it is decoded.
    entered: Vec<(NodeId, u8)>,
    /// Its spans and signatures, decoded when a body is first read.
    pub body_info: Option<BodyInfo>,
}

impl Loaded {
    /// A file whose symbols were entered from a symbol table built from the same bytes.
    pub fn undecoded(
        path: &str,
        bytes: Cow<'static, [u8]>,
        scala2: bool,
        entered: Vec<(NodeId, u8)>,
    ) -> Self {
        Self { path: path.to_owned(), bytes, scala2, decoded: OnceCell::new(), entered, body_info: None }
    }

    pub fn decoded_now(path: &str, bytes: Cow<'static, [u8]>, decoded: Decoded) -> Self {
        let loaded = Self::undecoded(path, bytes, decoded.scala2, Vec::new());
        let _ = loaded.decoded.set(decoded);
        loaded
    }

    /// Its trees and names, decoded on first use.
    pub fn decoded(&self) -> &Decoded {
        self.decoded.get_or_init(|| {
            let path = &self.path;
            let d = Decoded::parse(&self.bytes).unwrap_or_else(|e| {
                panic!("{path}: no longer decodes as when its symbol table was built: {e}")
            });
            let ast = &d.ast;
            let wrong =
                self.entered.iter().find(|&&(id, tag)| id.0 as usize >= ast.len() || ast.tag(id) != tag);
            if let Some((id, tag)) = wrong {
                panic!("{path}: symbol table entry at tree {} (tag {tag}) does not match the file", id.0);
            }
            d
        })
    }

    pub fn is_decoded(&self) -> bool {
        self.decoded.get().is_some()
    }
}
