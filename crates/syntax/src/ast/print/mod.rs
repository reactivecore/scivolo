//! Trees as S-expressions, in the format of `scripts/syntax-oracle` (its `tree` mode).

mod fields;

use super::constant::Constant;
use super::modifiers::Modifiers;
use super::span::Span;
use super::tree::{Clauses, List, MatchKind, NodeId, TName, Tree};
use super::{Ast, Attrs};
use crate::names::{Names, to_utf16};
use std::fmt::Write;

pub struct Printer<'a> {
    pub ast: &'a Ast,
    pub names: &'a Names,
    pub out: String,
}

/// `units` quoted, with escapes for quotes, backslashes, controls and lone surrogates.
pub fn quote(units: &[u16]) -> String {
    let mut q = String::from("\"");
    for c in char::decode_utf16(units.iter().copied()) {
        match c {
            Ok('"') => q.push_str("\\\""),
            Ok('\\') => q.push_str("\\\\"),
            Ok('\n') => q.push_str("\\n"),
            Ok('\r') => q.push_str("\\r"),
            Ok('\t') => q.push_str("\\t"),
            Ok(c) if c < ' ' => write!(q, "\\u{:04x}", c as u32).unwrap(),
            Ok(c) => q.push(c),
            Err(e) => write!(q, "\\u{:04x}", e.unpaired_surrogate()).unwrap(),
        }
    }
    q.push('"');
    q
}

pub fn span(s: Span) -> String {
    if !s.exists() {
        "-".into()
    } else if s.is_synthetic() {
        format!("{}..{}", s.start(), s.end())
    } else {
        format!("{}..{}@{}", s.start(), s.end(), s.point())
    }
}

impl Printer<'_> {
    fn name(&mut self, n: TName) {
        self.out.push(' ');
        self.bare_name(n);
    }

    fn bare_name(&mut self, n: TName) {
        let q = quote(&to_utf16(self.names.text(n.name)));
        write!(self.out, "{}:{}", if n.is_type { "T" } else { "t" }, &q[1..q.len() - 1]).unwrap();
    }

    fn opt(&mut self, t: &Option<NodeId>, empty: &str) {
        match t {
            Some(t) => {
                self.out.push(' ');
                self.tree(*t);
            }
            None => write!(self.out, " {empty}").unwrap(),
        }
    }

    fn node(&mut self, t: NodeId) {
        self.opt(&Some(t), "_");
    }

    fn list(&mut self, l: List) {
        self.out.push_str(" [");
        for &t in self.ast.items(l) {
            self.node(t);
        }
        self.out.push(']');
    }

    fn clauses(&mut self, c: Clauses) {
        self.out.push_str(" [");
        for &l in self.ast.clause_items(c) {
            self.list(l);
        }
        self.out.push(']');
    }

    fn mods(&mut self, m: &Modifiers) {
        write!(self.out, " (Mods [{}]", m.flags.flags_string()).unwrap();
        if let Some(w) = m.private_within {
            self.out.push_str(" within=");
            self.bare_name(TName { name: w, is_type: true });
        }
        self.list(m.annotations);
        self.out.push_str(" [");
        for md in &m.mods {
            write!(self.out, " ({} {})", md.kind.class_name(), span(md.span)).unwrap();
        }
        self.out.push_str("])");
    }

    fn constant(&mut self, c: &Constant) {
        match c {
            Constant::Unit | Constant::Null => write!(self.out, " #{}", c.tag()).unwrap(),
            _ => write!(self.out, " #{}:{}", c.tag(), quote(&c.string_value())).unwrap(),
        }
    }

    /// Print tree `id`.
    pub fn tree(&mut self, id: NodeId) {
        let node = self.ast.get(id);
        if node.tree == Tree::Empty {
            self.out.push('_');
            return;
        }
        let class =
            if node.attrs.contains(Attrs::XML_BLOCK) { "XMLBlock".into() } else { class_name(&node.tree) };
        write!(self.out, "({class} {}", span(self.ast.span(id))).unwrap();
        if node.attrs.contains(Attrs::BACKQUOTED) {
            self.out.push_str(" @backquoted");
        }
        if node.attrs.contains(Attrs::CAPTURE_VAR) {
            self.out.push_str(" @capturevar");
        }
        if node.attrs.contains(Attrs::RETAINS_ANNOT) {
            self.out.push_str(" @retains");
        }
        self.fields(&node.tree);
        self.out.push(')');
    }
}

/// dotty's class name of `tree`.
fn class_name(tree: &Tree) -> String {
    match tree {
        Tree::If { inline: true, .. } => "InlineIf".into(),
        Tree::Match { kind: MatchKind::Inline, .. } => "InlineMatch".into(),
        Tree::Match { kind: MatchKind::Sub, .. } => "SubMatch".into(),
        Tree::Template { derived: Some(_), .. } => "DerivingTemplate".into(),
        Tree::Function { mods: Some(_), .. } => "FunctionWithMods".into(),
        Tree::Function { wildcard: true, .. } => "WildcardFunction".into(),
        t => {
            let s = format!("{t:?}");
            s[..s.find([' ', '{', '(']).unwrap_or(s.len())].to_string()
        }
    }
}
