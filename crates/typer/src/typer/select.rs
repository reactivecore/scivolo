//! Selections `qual.name` and the members of a type (dotty's `typedSelect`, `Type.member`).

use super::{Pt, Typer};
use crate::ctx::Ctx;
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::{NodeId, TName};
use tasty::{Kind, SymbolId, Type, TypeId};

impl Typer<'_> {
    /// The members named `name` of type `t`.
    pub fn type_members(&mut self, t: TypeId, name: &str) -> Vec<SymbolId> {
        match self.u.types().get(t).clone() {
            Type::Package(p) => self.u.decl(p, name).unwrap_or_default(),
            Type::This(c) => self.type_members(c, name),
            Type::Sym(_, s) => match self.u.symbol(s).kind {
                Kind::Package => self.u.decl(s, name).unwrap_or_default(),
                Kind::Class => self.class_members(s, name),
                Kind::Val if self.u.symbol(s).flags.has(tasty::tags::OBJECT) => {
                    self.u.module_class(s).map_or_else(Vec::new, |c| self.class_members(c, name))
                }
                _ => {
                    self.complete(s);
                    let info = self.u.info(s);
                    self.type_members(info, name)
                }
            },
            _ => match self.u.class_symbol(t) {
                Some(c) => self.class_members(c, name),
                None => Vec::new(),
            },
        }
    }

    /// The members named `name` of class `c`, its base classes' included.
    pub fn class_members(&mut self, c: SymbolId, name: &str) -> Vec<SymbolId> {
        self.complete(c);
        let classes = self.u.linearization(c);
        for &base in classes.iter().skip(1) {
            self.complete(base);
        }
        self.u.member(c, name)
    }

    pub fn typed_select(&mut self, id: NodeId, qualifier: NodeId, name: TName, pt: Pt, ctx: Ctx) -> TNodeId {
        let qual = self.typed(qualifier, Pt::Any, ctx);
        let qual_tpe = self.tpd.tpe(qual);
        let text = self.text(name.name).to_owned();
        let syms: Vec<SymbolId> = self
            .type_members(qual_tpe, &text)
            .into_iter()
            .filter(|&s| self.is_type_symbol(s) == name.is_type)
            .collect();
        if syms.is_empty() {
            return self.unsupported(id, "member not found: extension or conversion");
        }
        let sym = match self.choose(&syms, pt) {
            Ok(sym) => sym,
            Err(what) => return self.unsupported(id, what),
        };
        if let Err(what) = self.check_reference(sym, ctx) {
            return self.unsupported(id, what);
        }
        let tpe = self.ref_type(Some(qual_tpe), sym);
        let span = self.ast.span(id);
        self.tpd.node(T::Select { qualifier: qual, name }, span, Some(tpe), Some(sym))
    }
}
