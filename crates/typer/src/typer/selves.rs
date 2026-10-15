//! Self definitions of classes, `self =>` and `this: S =>` (dotty's `selfInfo` in
//! `completeConstructor`, `ClassInfo.givenSelfType`).

use super::Typer;
use crate::ctx::Ctx;
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::flags as f;
use syntax::ast::{NodeId, Tree};
use tasty::{Kind, SymbolId, Type, TypeId};

impl Typer<'_> {
    /// Defines the self symbol of class `cls` from self definition `s`, its type left to
    /// `ensure_self` in the class's context `cctx`.
    pub fn complete_self(&mut self, cls: SymbolId, s: NodeId, cctx: Ctx) {
        let Tree::ValDef { name, .. } = *self.ast.tree(s) else { return };
        if self.u.is_module_class(cls) {
            self.unsupported(s, "named self of an object");
            return;
        }
        let text = self.text(name.name).to_owned();
        let sym = self.syms.define(self.u, &text, Kind::Val, cls, f::PRIVATE.or(f::LOCAL));
        self.u.unenter(sym);
        self.syms.by_tree.insert(s, sym);
        if text != "_" {
            self.syms.selves.insert(cls, sym);
        }
        self.syms.pending_selves.insert(cls, (s, cctx));
    }

    /// Types the self type of class `cls` if still pending and records it as its given self type
    /// unless the class type conforms to it.
    pub fn ensure_self(&mut self, cls: SymbolId) {
        let Some((s, cctx)) = self.syms.pending_selves.remove(&cls) else { return };
        let Tree::ValDef { tpt, .. } = *self.ast.tree(s) else { return };
        let typed = self.typed_type(tpt, cctx);
        self.typed_ahead(tpt, typed, cctx);
        let tpe = self.tpd.tpe(typed);
        self.u.set_info(self.syms.by_tree[&s], tpe);
        let applied = self.u.this_type(cls);
        if !self.u.is_subtype(applied, tpe) {
            self.u.set_self_type(cls, tpe);
        }
    }

    /// Self definition `s`, its type tree typed ahead in the class's context `cctx`; a type the
    /// source leaves out is an inferred one.
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

    /// Refuses member `sym` selected through `prefix`, not a `this`, if its info refers through
    /// its owner's `this` to a member of the owner's self type the owner does not inherit.
    pub fn check_self_members(&mut self, sym: SymbolId, prefix: Option<TypeId>) -> Result<(), &'static str> {
        let (Some(prefix), Some(owner)) = (prefix, self.u.symbol(sym).owner) else { return Ok(()) };
        self.ensure_self(owner);
        if self.u.given_self_type(owner).is_none() || matches!(self.u.types().get(prefix), Type::This(_)) {
            return Ok(());
        }
        self.complete(sym);
        let info = self.u.info(sym);
        let mut parts = Vec::new();
        self.u.map_type(info, &mut |u, part| {
            if let Type::Sym(Some(q), s) = *u.types().get(part)
                && let Type::This(c) = *u.types().get(q)
            {
                parts.push((c, s));
            }
            None
        });
        for (c, s) in parts {
            let inherited = self.u.symbol(s).owner.is_none_or(|o| self.u.derives_from(owner, o));
            if self.u.class_symbol(c) == Some(owner) && !inherited {
                return Err("member of a self type seen from outside its class");
            }
        }
        Ok(())
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
        let tpe = self.u.intern_type(Type::This(class));
        self.tpd.node(T::Ident { name }, self.ast.span(id), Some(tpe), Some(cls))
    }
}
