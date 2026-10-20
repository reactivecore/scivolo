// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/ContextOps.scala
//   compiler/src/dotty/tools/dotc/core/SymDenotations.scala
//   compiler/src/dotty/tools/dotc/core/Types.scala
//   compiler/src/dotty/tools/dotc/typer/Namer.scala
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

    /// Refuses class `cls` if its self type does not conform to a parent's given self type seen
    /// from `cls`, or that type still mentions the parent's `this` or type members.
    pub fn check_parent_selves(&mut self, cls: SymbolId) -> Result<(), &'static str> {
        let own = self.u.self_type(cls);
        let class = crate::defn::type_ref(self.u, cls);
        let this = self.u.intern_type(Type::This(class));
        for p in self.u.parents(cls) {
            let Some(pc) = self.u.class_symbol(p) else { continue };
            self.ensure_self(pc);
            let Some(s) = self.u.given_self_type(pc) else { continue };
            let s = self.u.as_seen_from(s, this, pc);
            let mut own_parts = false;
            self.u.map_type(s, &mut |u, part| {
                own_parts |= match *u.types().get(part) {
                    Type::This(c) => u.class_symbol(c) == Some(pc),
                    Type::Sym(_, t) => u.is_type_symbol(t) && u.symbol(t).owner == Some(pc),
                    _ => false,
                };
                None
            });
            let module = self.u.is_module_class(cls).then(|| self.u.source_module(cls)).flatten();
            let own_module = matches!(*self.u.types().get(s), Type::Sym(_, m) if Some(m) == module);
            if own_parts || !own_module && !self.u.is_subtype(own, s) {
                return Err("class not conforming to a parent's self type");
            }
        }
        Ok(())
    }

    /// The type a definition infers from `this` type `t` of class type `c`: `t` for an object, the
    /// self type for a class with a given one, else `c` (dotty's `ThisType.underlying`,
    /// `ClassInfo.selfType`).
    pub fn widened_this(&mut self, t: TypeId, c: TypeId) -> TypeId {
        let Some(k) = self.u.class_symbol(c) else { return c };
        if self.u.is_module_class(k) {
            return t;
        }
        self.ensure_self(k);
        match self.u.given_self_type(k) {
            Some(_) => self.u.self_type(k),
            None => c,
        }
    }

    /// Whether `ctx` is the parent call context of a class with parent class `c`, or of a class
    /// whose parents are being typed.
    pub fn is_parent_call(&mut self, c: SymbolId, ctx: Ctx) -> bool {
        let Some(k) = self.u.symbol(ctx.owner).owner else { return false };
        if self.syms.primary_ctors.get(&k) != Some(&ctx.owner) {
            return false;
        }
        let parents = self.u.parents(k);
        parents.is_empty() || parents.into_iter().any(|p| self.u.class_symbol(p) == Some(c))
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

    /// The self symbol of class `cls` if `name` names it (dotty's `denotNamed` in `inClassContext`).
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
