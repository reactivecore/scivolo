// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/inlines/PrepareInlineable.scala
//   compiler/src/dotty/tools/dotc/transform/AccessProxies.scala
//! Inline accessors: the members an inline method's body refers to that are not public where it
//! expands, reached through public synthetic methods of a class that sees them (dotty's
//! `PrepareInlineable.MakeInlineableDirect`, `AccessProxies`).

use super::Typer;
use crate::ctx::Ctx;
use crate::tpd::{TNodeId, Tree as T};
use rustc_hash::{FxHashMap, FxHashSet};
use syntax::ast::{NodeId, Span};
use tasty::{SymbolId, Type};

#[derive(Default)]
pub struct Accessors {
    /// Each accessor made, by its class, the member it accesses and whether it is a setter.
    made: FxHashMap<(SymbolId, SymbolId, bool), SymbolId>,
    /// The definitions of the accessors still to type, by class, with the context to type them in.
    pending: FxHashMap<SymbolId, Vec<(NodeId, Ctx)>>,
    /// The classes whose bodies are typed.
    closed: FxHashSet<SymbolId>,
}

impl Typer<'_> {
    /// Class body `body` of `cls`, typed in `ctx`, with the body retainers of its retained inline
    /// methods and its inline accessors (dotty's `inlineExpansion`, `addAccessorDefs`).
    pub fn with_inline_members(&mut self, cls: SymbolId, body: Vec<TNodeId>, ctx: Ctx) -> Vec<TNodeId> {
        let mut body = self.with_retainers(body, ctx);
        for (def, def_ctx) in self.accessors.pending.remove(&cls).unwrap_or_default() {
            body.push(self.typed_stat(def, def_ctx));
        }
        self.accessors.closed.insert(cls);
        body
    }

    /// Replaces in `t`, in the body of inline method `meth`, the references that need inline
    /// accessors by references to them, and the assignments to such variables by setter calls.
    pub fn make_inlineable(&mut self, t: TNodeId, meth: SymbolId, ctx: Ctx) {
        if let T::Assign { lhs, rhs } = *self.tpd.tree(t)
            && let Some(var) = self.tpd.sym(lhs)
            && self.needs_inline_accessor_from(var, meth)
        {
            for c in self.tpd.children(lhs) {
                self.make_inlineable(c, meth, ctx);
            }
            self.make_inlineable(rhs, meth, ctx);
            if self.rewire(lhs, var, ctx).is_some() {
                let span = self.tpd.span(lhs);
                let Some(setter) = self.inline_accessor(var, true, span, ctx) else { return };
                let fun = self.retarget(lhs, setter, span);
                let unit = self.tpd.tpe(t);
                let args = self.tpd.list(&[rhs]);
                let node = self.tpd.get_mut(t);
                (node.tree, node.tpe) = (T::Apply { fun, args }, Some(unit));
            }
            return;
        }
        let before = self.tpd.get(t).tpe;
        let block_expr = match *self.tpd.tree(t) {
            T::Block { expr, .. } => Some((expr, self.tpd.get(expr).tpe)),
            _ => None,
        };
        for c in self.tpd.children(t) {
            self.make_inlineable(c, meth, ctx);
        }
        if let Some((expr, old)) = block_expr
            && old == before
        {
            self.tpd.get_mut(t).tpe = self.tpd.get(expr).tpe;
        }
        if let T::Select { qualifier, .. } = *self.tpd.tree(t)
            && let Some(tpe) = before
            && let Type::Sym(Some(_), m) = *self.u.types().get(tpe)
            && self.rewired(qualifier)
        {
            let accessor = self.tpd.sym(qualifier).expect("an accessor");
            self.complete(accessor);
            let mut prefix = self.u.info(accessor);
            if let Type::ByName(r) = *self.u.types().get(prefix) {
                prefix = r;
            }
            self.tpd.get_mut(t).tpe = Some(self.u.intern_type(Type::Sym(Some(prefix), m)));
        }
        if matches!(self.tpd.tree(t), T::Ident { .. } | T::Select { .. })
            && let Some(sym) = self.tpd.sym(t)
            && self.needs_inline_accessor_from(sym, meth)
        {
            self.rewire(t, sym, ctx);
        }
    }

    /// Whether `t` refers to an inline accessor.
    fn rewired(&self, t: TNodeId) -> bool {
        self.tpd.sym(t).is_some_and(|s| self.accessors.made.values().any(|&a| a == s))
    }

    /// Makes reference `t` to `sym` refer to its accessor; the accessor.
    fn rewire(&mut self, t: TNodeId, sym: SymbolId, ctx: Ctx) -> Option<SymbolId> {
        let span = self.tpd.span(t);
        if self.u.is_constructor(sym) {
            self.unsupported_at(span, "private constructor in an inline method");
            return None;
        }
        let accessor = self.inline_accessor(sym, false, span, ctx)?;
        let new = self.retarget(t, accessor, span);
        let n = self.tpd.get(new);
        let (tree, tpe, sym) = (n.tree.clone(), n.tpe, n.sym);
        let node = self.tpd.get_mut(t);
        (node.tree, node.tpe, node.sym) = (tree, tpe, sym);
        Some(accessor)
    }

    /// A reference to `accessor` in place of reference `t`: a selection from its qualifier if
    /// the qualifier's type derives from the accessor's class (dotty's `rewire`).
    fn retarget(&mut self, t: TNodeId, accessor: SymbolId, span: Span) -> TNodeId {
        let host = self.u.symbol(accessor).owner.expect("a member");
        if let T::Select { qualifier, .. } = *self.tpd.tree(t)
            && let widened = self.u.widen(self.tpd.tpe(qualifier))
            && let Some(c) = self.u.class_symbol(widened)
            && self.u.derives_from(c, host)
        {
            return self.build().select(qualifier, accessor, span);
        }
        self.member_ref(accessor, span)
    }

    /// The accessor of `accessed`, a setter if `setter`, for a reference at `span` in `ctx`, made
    /// and entered unless it exists (dotty's `useAccessor`); `None` if it cannot be made.
    fn inline_accessor(
        &mut self,
        accessed: SymbolId,
        setter: bool,
        span: Span,
        ctx: Ctx,
    ) -> Option<SymbolId> {
        let Some(host) = self.accessor_host(accessed, ctx.owner) else {
            self.unsupported_at(span, "non-public member in an inline method without an accessor class");
            return None;
        };
        if let Some(&made) = self.accessors.made.get(&(host, accessed, setter)) {
            return Some(made);
        }
        let owner = self.u.symbol(accessed).owner.expect("a member");
        let generic_base = owner != host && !self.u.type_params(owner).is_empty();
        if self.accessors.closed.contains(&host) {
            self.unsupported_at(span, "inline accessor in a class whose body is typed already");
            return None;
        }
        if generic_base {
            self.unsupported_at(span, "inline accessor of a member of a generic base class");
            return None;
        }
        let name = self.accessor_name(accessed, host, setter);
        let def = self.accessor_def(&name, host, accessed, setter, span)?;
        let actx = Ctx { owner: host, ..ctx };
        self.index(&[def], actx);
        let sym = self.syms.by_tree[&def];
        self.accessors.made.insert((host, accessed, setter), sym);
        self.accessors.pending.entry(host).or_default().push((def, actx));
        Some(sym)
    }
}
