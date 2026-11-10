// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/inlines/PrepareInlineable.scala
//   compiler/src/dotty/tools/dotc/transform/AccessProxies.scala
//! References and assignments made through inline accessors (dotty's `MakeInlineableDirect`,
//! `useAccessor` and `rewire`).

use super::super::Typer;
use super::accessor_defs::{accessor_def, accessor_host};
use super::{Access, MakeInlineable};
use crate::ctx::Ctx;
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::Span;
use tasty::SymbolId;

impl MakeInlineable {
    /// Rewrites assignment `a` to variable `var`, which needs an accessor: `x = e` becomes
    /// `inline$x_=(e)`, or a call of a setter that takes the receiver if no class around the
    /// method can host the accessor.
    pub fn assignment(&self, t: &mut Typer<'_>, a: TNodeId, var: SymbolId) {
        let T::Assign { lhs, rhs } = *t.tpd.tree(a) else { return };
        if self.pass_receiver(t, lhs, var) {
            for c in t.tpd.children(a) {
                self.run(t, c);
            }
            self.pass_setter(t, a, var);
            return;
        }

        for c in t.tpd.children(lhs) {
            self.run(t, c);
        }
        self.run(t, rhs);
        if self.rewire(t, lhs, var).is_some() {
            let span = t.tpd.span(lhs);
            let Some(setter) = self.inline_accessor(t, Access { member: var, setter: true }, span) else {
                return;
            };
            let fun = retarget(t, lhs, setter, span);
            let unit = t.tpd.tpe(a);
            let args = t.tpd.list(&[rhs]);
            let node = t.tpd.get_mut(a);
            (node.tree, node.tpe) = (T::Apply { fun, args }, Some(unit));
        }
    }

    /// Makes reference `r` to `sym` refer to its accessor; the accessor.
    pub fn rewire(&self, t: &mut Typer<'_>, r: TNodeId, sym: SymbolId) -> Option<SymbolId> {
        let span = t.tpd.span(r);
        if t.u.is_constructor(sym) {
            t.unsupported_at(span, "private constructor in an inline method");
            return None;
        }
        let accessor = self.inline_accessor(t, Access { member: sym, setter: false }, span)?;
        let new = retarget(t, r, accessor, span);
        let n = t.tpd.get(new);
        let (tree, tpe, sym) = (n.tree.clone(), n.tpe, n.sym);
        let node = t.tpd.get_mut(r);
        (node.tree, node.tpe, node.sym) = (tree, tpe, sym);
        Some(accessor)
    }

    /// The accessor of `access` for a reference at `span`, made and entered unless it exists
    /// (dotty's `useAccessor`); `None` if no class can host it or it cannot be made.
    fn inline_accessor(&self, t: &mut Typer<'_>, access: Access, span: Span) -> Option<SymbolId> {
        let host = accessor_host(t.u, access.member, self.ctx.owner)?;
        if let Some(&made) = t.accessors.made.get(&(host, access)) {
            return Some(made);
        }
        let owner = t.u.symbol(access.member).owner.expect("a member");
        let generic_base = owner != host && !t.u.type_params(owner).is_empty();
        if t.accessors.closed.contains(&host) {
            t.unsupported_at(span, "inline accessor in a class whose body is typed already");
            return None;
        }
        if generic_base {
            t.unsupported_at(span, "inline accessor of a member of a generic base class");
            return None;
        }

        let def = accessor_def(t, host, access, span)?;
        let actx = Ctx { owner: host, ..self.ctx };
        t.index(&[def], actx);
        let sym = t.syms.by_tree[&def];
        t.accessors.made.insert((host, access), sym);
        t.accessors.pending.entry(host).or_default().push((def, actx));
        Some(sym)
    }
}

/// A reference to `accessor` in place of reference `r`: a selection from its qualifier if the
/// qualifier's type derives from the accessor's class (dotty's `rewire`).
fn retarget(t: &mut Typer<'_>, r: TNodeId, accessor: SymbolId, span: Span) -> TNodeId {
    let host = t.u.symbol(accessor).owner.expect("a member");
    if let T::Select { qualifier, .. } = *t.tpd.tree(r)
        && let widened = t.u.widen(t.tpd.tpe(qualifier))
        && let Some(c) = t.u.class_symbol(widened)
        && t.u.derives_from(c, host)
    {
        return t.build().select(qualifier, accessor, span);
    }
    t.member_ref(accessor, span)
}
