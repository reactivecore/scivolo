// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/inlines/PrepareInlineable.scala
//   compiler/src/dotty/tools/dotc/transform/AccessProxies.scala
//! References through inline accessors that take the accessed member's receiver as their first
//! parameter (dotty's `MakeInlineablePassing`, `useSetter`).

use super::super::Typer;
use super::accessor_defs::accessor_host;
use super::passing_defs::{dealiased, passing_def};
use super::{Access, MakeInlineable};
use crate::ctx::Ctx;
use crate::tpd::{TNodeId, Tree as T};
use names::name_kinds;
use syntax::ast::Span;
use tasty::{Kind, SymbolId, Type, TypeId, Universe};

/// The receiver a passing accessor takes: its type, aliases expanded, and the types defined in
/// the inline method that it refers to, which become the accessor's type parameters.
pub struct Receiver {
    /// The widened type of the selection's qualifier, aliases expanded.
    pub tpe: TypeId,
    /// The types defined in the inline method that `tpe` refers to.
    pub locals: Vec<SymbolId>,
}

impl MakeInlineable {
    /// Replaces selection `s` of `sym`, which no class around the method can host an accessor
    /// of, by a call of an accessor in the method's class that takes the selection's qualifier:
    /// `c.x` becomes `inline$x$i1(c)`; whether it did (dotty's `MakeInlineablePassing`).
    pub fn pass_receiver(&self, t: &mut Typer<'_>, s: TNodeId, sym: SymbolId) -> bool {
        let T::Select { qualifier, .. } = *t.tpd.tree(s) else { return false };
        if t.u.is_constructor(sym) || accessor_host(t.u, sym, self.ctx.owner).is_some() {
            return false;
        }
        let Some(host) = passing_host(t.u, self.meth) else { return false };
        let span = t.tpd.span(s);
        let receiver = self.receiver(t, qualifier);
        let name = t.u.symbol(sym).name.clone();
        let unique = t.fresh_name(span, &format!("{name}$i"));
        let suffix = t.names.text(unique)[name.len()..].to_owned();
        let getter = name_kinds::inline_accessor(&format!("{name}{suffix}"));
        let access = Access { member: sym, setter: false };
        let Some(getter) = self.passing_accessor(t, host, &getter, access, &receiver, span) else {
            return false;
        };
        let Some((fun, info)) = passing_fun(t, getter, &receiver.locals, Span::at(span.start())) else {
            return false;
        };

        let Some(result) = t.u.method(info).map(|m| m.result) else { return false };
        let method = t.u.method(result).is_some();
        let end = t.tpd.span(qualifier).end();
        let args = t.tpd.list(&[qualifier]);
        let node = t.tpd.get_mut(s);
        (node.tree, node.tpe, node.sym) = (T::Apply { fun, args }, Some(result), None);
        if method {
            node.span = Span::synthetic(span.start(), end);
        }
        true
    }

    /// Replaces assignment `a` to `var` through the passing getter `pass_receiver` made of its
    /// left-hand side by a call of the setter of that name: `inline$x$i1(c) = e` becomes
    /// `inline$x_=$i1(c)(e)` (dotty's `useSetter`).
    pub fn pass_setter(&self, t: &mut Typer<'_>, a: TNodeId, var: SymbolId) {
        let T::Assign { lhs, rhs } = *t.tpd.tree(a) else { return };
        let T::Apply { fun, args } = *t.tpd.tree(lhs) else { return };
        let (Some(getter), Some(host)) = (t.tpd.sym(t.tpd.meth_part(fun)), passing_host(t.u, self.meth))
        else {
            return;
        };
        let qualifier = t.tpd.items(args)[0];
        let receiver = self.receiver(t, qualifier);
        let prefix = name_kinds::inline_accessor(&t.u.symbol(var).name);
        let setter = format!("{prefix}_={}", &t.u.symbol(getter).name[prefix.len()..]);
        let span = t.tpd.span(lhs);
        let access = Access { member: var, setter: true };
        let Some(setter) = self.passing_accessor(t, host, &setter, access, &receiver, span) else { return };
        let Some((fun, info)) = passing_fun(t, setter, &receiver.locals, Span::at(span.start())) else {
            return;
        };

        let Some(result) = t.u.method(info).map(|m| m.result) else { return };
        let node = t.tpd.get_mut(lhs);
        (node.tree, node.tpe) = (T::Apply { fun, args }, Some(result));
        let args = t.tpd.list(&[rhs]);
        t.tpd.get_mut(a).tree = T::Apply { fun: lhs, args };
    }

    /// The receiver of a passing accessor for `qualifier`.
    fn receiver(&self, t: &mut Typer<'_>, qualifier: TNodeId) -> Receiver {
        let widened = t.u.widen(t.tpd.tpe(qualifier));
        let tpe = dealiased(t.u, widened);
        Receiver { tpe, locals: local_refs(t.u, tpe, self.meth) }
    }

    /// The passing accessor `name` of `access` for `receiver`, entered in class `host` for a
    /// reference at `span`.
    fn passing_accessor(
        &self,
        t: &mut Typer<'_>,
        host: SymbolId,
        name: &str,
        access: Access,
        receiver: &Receiver,
        span: Span,
    ) -> Option<SymbolId> {
        if t.accessors.closed.contains(&host) {
            t.unsupported_at(span, "inline accessor in a class whose body is typed already");
            return None;
        }
        let def = passing_def(t, name, access, receiver, span)?;
        let actx = Ctx { owner: host, ..self.ctx };
        t.index(&[def], actx);
        let sym = t.syms.by_tree[&def];
        t.accessors.pending.entry(host).or_default().push((def, actx));
        if access.setter {
            t.complete(sym);
            let info = t.u.info(sym);
            let info = with_receiver_named(t.u, info);
            t.u.set_info(sym, info);
        }
        Some(sym)
    }
}

/// The class of inline method `meth` that hosts its passing accessors.
fn passing_host(u: &Universe, meth: SymbolId) -> Option<SymbolId> {
    u.symbol(meth).owner.filter(|&c| u.symbol(c).kind == Kind::Class)
}

/// A reference to accessor `target` at `at`, applied to the types `locals` if any, and its
/// method type.
fn passing_fun(
    t: &mut Typer<'_>,
    target: SymbolId,
    locals: &[SymbolId],
    at: Span,
) -> Option<(TNodeId, TypeId)> {
    t.complete(target);
    let fun = t.member_ref(target, at);
    if locals.is_empty() {
        return Some((fun, t.u.info(target)));
    }
    let targs: Vec<TypeId> = locals.iter().map(|&l| crate::defn::type_ref(t.u, l)).collect();
    let tpe = t.instantiated(fun, &targs).ok()?;
    let trees: Vec<TNodeId> = targs.iter().map(|&a| t.type_tree(a, at, false)).collect();
    Some((t.build().type_apply(fun, &trees, tpe, at), tpe))
}

/// Accessor type `info` with its receiver parameter named `x$0`.
fn with_receiver_named(u: &mut Universe, info: TypeId) -> TypeId {
    match *u.ty(info) {
        Type::Poly { binder, params, result } => {
            let result = with_receiver_named(u, result);
            u.intern_type(Type::Poly { binder, params, result })
        }
        Type::Method { binder, params, result, implicit, contextual } => {
            let mut params = u.params_of(params).to_vec();
            params[0].0 = u.intern_name(&name_kinds::synthetic_param(0));
            u.intern_method(binder, &params, result, implicit, contextual)
        }
        _ => info,
    }
}

/// The types defined in method `meth` that type `t` refers to, in the reverse order of their
/// first occurrence (dotty's `namedPartsWith`).
fn local_refs(u: &mut Universe, t: TypeId, meth: SymbolId) -> Vec<SymbolId> {
    let mut found = Vec::new();
    u.map_type(t, &mut |u, p| {
        if let Some(s) = u.sym_of(p)
            && u.symbol(s).kind.is_type()
            && crate::sealed::is_contained_in(u, s, meth)
            && !found.contains(&s)
        {
            found.push(s);
        }
        None
    });
    found.reverse();
    found
}
