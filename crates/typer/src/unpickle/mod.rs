// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/tasty/TreeUnpickler.scala
//   compiler/src/dotty/tools/dotc/typer/TypeAssigner.scala
//! dotty's `TreeUnpickler.readTree` for the bodies of the library's inline methods: TASTy trees
//! read into typed trees, typed as `TypeAssigner` types them, in the spans of the library's
//! source. Bodies outside the subset read are refused.

mod admitted;
mod assign;
mod cases;
mod defs;
mod locals;
mod paths;
mod quote_patterns;
mod quotes;
mod selections;
mod splices;
mod terms;
mod tpts;

use crate::tpd::{TNodeId, Tpd, Tree as T};
use syntax::ast::{Span, TName};
use syntax::names::Names;
use tasty::{Ast, FileId, NameRef, NodeId, SymbolId, TypeId, Universe};

pub use quote_patterns::QuotePattern;
pub use quotes::{Quote, QuoteHole, Unpickled, quote, quoted_type};

/// Why a body is outside the subset read.
pub type Refusal = &'static str;

pub struct Unpickler<'a> {
    pub u: &'a mut Universe,
    pub tpd: &'a mut Tpd,
    pub names: &'a mut Names,
    file: FileId,
    /// The inline method whose body is read.
    meth: SymbolId,
    /// The owner of the local definitions read.
    owner: SymbolId,
    /// The term holes of the quote read, if a quote is read.
    holes: Option<Vec<quotes::QuoteHole>>,
}

/// The definition of inline method `meth` of the library, with its body read from its TASTy.
pub fn inline_def(
    u: &mut Universe,
    tpd: &mut Tpd,
    names: &mut Names,
    meth: SymbolId,
) -> Result<TNodeId, Refusal> {
    let Some((file, id)) = u.symbol(meth).tree else { return Err("inline method without TASTy") };
    if u.body_info(file).is_err() {
        return Err("inline method of an unreadable TASTy file");
    }
    define_repeated(u)?;
    let mut unpickler = Unpickler { u, tpd, names, file, meth, owner: meth, holes: None };
    unpickler.inline_def(id)
}

/// Defines the class `scala.<repeated>` of repeated parameter types, which pickled trees name.
fn define_repeated(u: &mut Universe) -> Result<(), Refusal> {
    let scala = u.package("scala").ok_or("a library without package scala")?;
    let synthetic = tasty::Flags::default().with(tasty::tags::SYNTHETIC);
    crate::defn::special(u, scala, "<repeated>", tasty::Kind::Class, synthetic);
    Ok(())
}

impl Unpickler<'_> {
    fn ast(&self) -> &Ast {
        self.u.file(self.file).0
    }

    fn tag(&self, id: NodeId) -> u8 {
        self.ast().tag(id)
    }

    fn kids(&self, id: NodeId) -> Vec<NodeId> {
        self.ast().kids(id).to_vec()
    }

    /// The name the tree at `id` holds, shown without a signature.
    fn name(&self, id: NodeId) -> String {
        let (ast, names) = self.u.file(self.file);
        names.get(ast.node(id).a as usize).cloned().unwrap_or_default()
    }

    fn tname(&mut self, id: NodeId, is_type: bool) -> TName {
        let text = self.name(id);
        TName { name: self.names.intern(&text), is_type }
    }

    /// The parameter signature of the name the tree at `id` holds, if it is a signed name.
    fn signature(&mut self, id: NodeId) -> Option<Vec<String>> {
        let name = NameRef(self.ast().node(id).a as u32);
        self.u.body_info(self.file).ok()?.signature(name).map(<[String]>::to_vec)
    }

    /// The type of the type, type tree or path at `id`.
    fn ty(&mut self, id: NodeId) -> TypeId {
        self.u.type_at(self.file, id)
    }

    /// A tree without a span yet.
    fn bare(&mut self, tree: T, tpe: Option<TypeId>, sym: Option<SymbolId>) -> TNodeId {
        self.tpd.node(tree, Span::NONE, tpe, sym)
    }

    /// Gives `t`, read from `id`, the span pickled for `id`, else the one it has, else the
    /// envelope of its children's (dotty's `setSpan` over `Positioned.envelope`).
    fn positioned(&mut self, id: NodeId, t: TNodeId) -> TNodeId {
        let addr = self.ast().node(id).addr;
        let pickled = self.u.body_info(self.file).ok().and_then(|i| i.span(addr)).map(|s| {
            match s.point {
                Some(p) => Span::new(s.start, s.end, p),
                None => Span::synthetic(s.start, s.end),
            }
            .in_other_source()
        });
        let envelope = self.envelope(t, Span::NONE);
        let span = match pickled {
            Some(s) => s,
            None if self.tpd.span(t).exists() => return t,
            None => envelope,
        };
        self.tpd.get_mut(t).span = span;
        t
    }

    /// The span enclosing `t`'s children from `start` on, a child without a span given the end
    /// of the spans before it (dotty's `Positioned.envelope`).
    fn envelope(&mut self, t: TNodeId, start: Span) -> Span {
        if let T::Inlined { call, .. } = self.tpd.tree(t) {
            return call.map_or(Span::NONE, |c| self.tpd.span(c));
        }
        let children = self.tpd.children(t);
        let mut span = start;
        let mut left = false;
        for pass in 0..2 {
            for c in children.clone() {
                let s = self.tpd.span(c);
                if s.exists() {
                    span = span.union(s);
                } else if span.exists() {
                    let placed = self.envelope(c, span.end_pos());
                    self.tpd.get_mut(c).span = placed;
                } else {
                    left = true;
                }
            }
            if pass > 0 || !left || !span.exists() {
                break;
            }
            span = span.start_pos();
        }
        span.to_synthetic()
    }
}
