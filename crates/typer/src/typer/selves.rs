//! Self definitions of classes, `self =>` and `this: S =>` (dotty's `selfInfo` in
//! `completeConstructor`, `ClassInfo.givenSelfType`).

use super::Typer;
use crate::ctx::Ctx;
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::flags as f;
use syntax::ast::{NodeId, Tree};
use tasty::{Kind, SymbolId};

impl Typer<'_> {
    /// Defines the self symbol of class `cls` from self definition `s`, typed in the class's
    /// context `cctx`, and records its given self type.
    pub fn complete_self(&mut self, cls: SymbolId, s: NodeId, cctx: Ctx) {
        let Tree::ValDef { name, tpt, .. } = *self.ast.tree(s) else { return };
        let typed = self.typed_type(tpt, cctx);
        self.typed_ahead(tpt, typed, cctx);
        let tpe = self.tpd.tpe(typed);
        let text = self.text(name.name).to_owned();
        let sym = self.syms.define(self.u, &text, Kind::Val, cls, f::PRIVATE.or(f::LOCAL));
        self.u.unenter(sym);
        self.u.set_info(sym, tpe);
        self.syms.by_tree.insert(s, sym);
        if text != "_" {
            self.syms.selves.insert(cls, sym);
        }
        if tpe != self.u.this_type(cls) {
            self.u.set_self_type(cls, tpe);
        }
    }

    /// The self definition `s` of class `cls`, its type tree typed ahead in `cctx`.
    pub fn typed_self(&mut self, s: NodeId, cctx: Ctx) -> TNodeId {
        let Tree::ValDef { name, tpt, .. } = *self.ast.tree(s) else { unreachable!("a self definition") };
        let typed = match self.ready(tpt, cctx) {
            Some(t) => t,
            None => self.typed_type(tpt, cctx),
        };
        let span = self.ast.span(tpt);
        let tpt = match span.start() == span.end() {
            true => {
                let tpe = self.tpd.tpe(typed);
                self.type_tree(tpe, span, true)
            }
            false => typed,
        };
        let sym = self.syms.by_tree.get(&s).copied();
        self.tpd.node(T::ValDef { name, tpt, rhs: None }, self.ast.span(s), None, sym)
    }

    /// The self symbol of class `cls` if `name` names it.
    pub fn self_named(&self, cls: SymbolId, name: &str) -> Option<SymbolId> {
        self.syms.selves.get(&cls).copied().filter(|&s| self.u.symbol(s).name == name)
    }

    /// A reference to the self of the class owning self symbol `sym`: an identifier of the class
    /// typed `C.this`.
    pub fn self_ref(&mut self, id: NodeId, name: syntax::ast::TName, sym: SymbolId) -> TNodeId {
        let cls = self.u.symbol(sym).owner.expect("a class owns its self");
        let class = crate::defn::type_ref(self.u, cls);
        let tpe = self.u.intern_type(tasty::Type::This(class));
        self.tpd.node(T::Ident { name }, self.ast.span(id), Some(tpe), Some(cls))
    }
}
