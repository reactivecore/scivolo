//! dotty's `TreeUnpickler.readTree` for the bodies of the library's inline methods: TASTy trees
//! read into typed trees, typed as `TypeAssigner` types them, in the spans of the library's
//! source. Bodies outside the subset read are refused.

mod admitted;
mod assign;
mod defs;
mod paths;
mod selections;
mod terms;
mod tpts;

use crate::tpd::{TNodeId, Tpd, Tree as T};
use syntax::ast::{Span, TName};
use syntax::names::Names;
use tasty::{Ast, FileId, NameRef, NodeId, SymbolId, TypeId, Universe};

/// Why a body is outside the subset read.
pub type Refusal = &'static str;

pub struct Unpickler<'a> {
    pub u: &'a mut Universe,
    pub tpd: &'a mut Tpd,
    pub names: &'a mut Names,
    file: FileId,
    /// The inline method whose body is read.
    meth: SymbolId,
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
    Unpickler { u, tpd, names, file, meth }.inline_def(id)
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
        let span = match pickled {
            Some(s) => s,
            None if self.tpd.span(t).exists() => return t,
            None => {
                let children = self.tpd.children(t);
                children.iter().fold(Span::NONE, |s, &c| s.union(self.tpd.span(c))).to_synthetic()
            }
        };
        self.tpd.get_mut(t).span = span;
        t
    }
}
