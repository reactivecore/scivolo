// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/inlines/PrepareInlineable.scala
//   compiler/src/dotty/tools/dotc/transform/AccessProxies.scala
//! References through inline accessors that take the accessed member's receiver as their first
//! parameter (dotty's `MakeInlineablePassing`, `useSetter`).

use super::super::Typer;
use super::passing_defs::{dealiased, local_refs, with_receiver_named};
use crate::ctx::Ctx;
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::Span;
use tasty::{Kind, SymbolId, Type, TypeId};

impl Typer<'_> {
    /// Replaces selection `t` of `sym`, which no class around `ctx.owner` can host an accessor
    /// of, by a call of an accessor in the class of inline method `meth` that takes the
    /// selection's qualifier; whether it did (dotty's `MakeInlineablePassing`).
    pub fn pass_receiver(&mut self, t: TNodeId, sym: SymbolId, meth: SymbolId, ctx: Ctx) -> bool {
        let T::Select { qualifier, .. } = *self.tpd.tree(t) else { return false };
        if self.u.is_constructor(sym) || self.accessor_host(sym, ctx.owner).is_some() {
            return false;
        }
        let Some(host) = self.passing_host(meth) else { return false };
        let span = self.tpd.span(t);
        let (receiver, locals) = self.passed_receiver(qualifier, meth);
        let name = self.u.symbol(sym).name.clone();
        let unique = self.fresh.next(self.names, &format!("{name}$i"));
        let suffix = self.names.text(unique)[name.len()..].to_owned();
        let getter = format!("inline${name}{suffix}");
        let recv = (receiver, &locals[..]);
        let Some(getter) = self.passing_accessor(host, &getter, sym, recv, false, span, ctx) else {
            return false;
        };
        let Some((fun, info)) = self.passing_fun(getter, &locals, Span::at(span.start())) else {
            return false;
        };
        let Type::Method { result, .. } = *self.u.types().get(info) else { return false };
        let method = matches!(self.u.types().get(result), Type::Method { .. });
        let end = self.tpd.span(qualifier).end();
        let args = self.tpd.list(&[qualifier]);
        let node = self.tpd.get_mut(t);
        (node.tree, node.tpe, node.sym) = (T::Apply { fun, args }, Some(result), None);
        if method {
            node.span = Span::synthetic(span.start(), end);
        }
        true
    }

    /// Replaces assignment `a` to `var` through the passing getter `pass_receiver` made of its
    /// left-hand side by a call of the setter of that name (dotty's `useSetter`).
    pub fn pass_setter(&mut self, a: TNodeId, var: SymbolId, meth: SymbolId, ctx: Ctx) {
        let T::Assign { lhs, rhs } = *self.tpd.tree(a) else { return };
        let T::Apply { fun, args } = *self.tpd.tree(lhs) else { return };
        let (Some(getter), Some(host)) = (self.tpd.sym(self.tpd.meth_part(fun)), self.passing_host(meth))
        else {
            return;
        };
        let qualifier = self.tpd.items(args)[0];
        let (receiver, locals) = self.passed_receiver(qualifier, meth);
        let prefix = format!("inline${}", self.u.symbol(var).name);
        let setter = format!("{prefix}_={}", &self.u.symbol(getter).name[prefix.len()..]);
        let span = self.tpd.span(lhs);
        let recv = (receiver, &locals[..]);
        let Some(setter) = self.passing_accessor(host, &setter, var, recv, true, span, ctx) else { return };
        let Some((fun, info)) = self.passing_fun(setter, &locals, Span::at(span.start())) else { return };
        let Type::Method { result, .. } = *self.u.types().get(info) else { return };
        let node = self.tpd.get_mut(lhs);
        (node.tree, node.tpe) = (T::Apply { fun, args }, Some(result));
        let args = self.tpd.list(&[rhs]);
        self.tpd.get_mut(a).tree = T::Apply { fun: lhs, args };
    }

    /// The class of inline method `meth` that hosts its passing accessors.
    fn passing_host(&self, meth: SymbolId) -> Option<SymbolId> {
        self.u.symbol(meth).owner.filter(|&c| self.u.symbol(c).kind == Kind::Class)
    }

    /// The receiver type of a passing accessor for `qualifier` and the types of inline method
    /// `meth` it refers to.
    fn passed_receiver(&mut self, qualifier: TNodeId, meth: SymbolId) -> (TypeId, Vec<SymbolId>) {
        let widened = self.u.widen(self.tpd.tpe(qualifier));
        let receiver = dealiased(self.u, widened);
        (receiver, local_refs(self.u, receiver, meth))
    }

    /// A reference to accessor `target` at `at`, applied to the types `locals` if any, and its
    /// method type.
    fn passing_fun(&mut self, target: SymbolId, locals: &[SymbolId], at: Span) -> Option<(TNodeId, TypeId)> {
        self.complete(target);
        let fun = self.member_ref(target, at);
        if locals.is_empty() {
            return Some((fun, self.u.info(target)));
        }
        let targs: Vec<TypeId> = locals.iter().map(|&l| crate::defn::type_ref(self.u, l)).collect();
        let tpe = self.instantiated(fun, &targs).ok()?;
        let trees: Vec<TNodeId> = targs.iter().map(|&a| self.type_tree(a, at, false)).collect();
        Some((self.build().type_apply(fun, &trees, tpe, at), tpe))
    }

    /// The passing accessor `name` of `accessed` for receivers of type `receiver` over the types
    /// of the inline method it names, a setter if `setter`, entered in class `host` for a
    /// reference at `span` in `ctx`.
    #[allow(clippy::too_many_arguments)]
    pub fn passing_accessor(
        &mut self,
        host: SymbolId,
        name: &str,
        accessed: SymbolId,
        receiver: (TypeId, &[SymbolId]),
        setter: bool,
        span: Span,
        ctx: Ctx,
    ) -> Option<SymbolId> {
        if self.accessors.closed.contains(&host) {
            self.unsupported_at(span, "inline accessor in a class whose body is typed already");
            return None;
        }
        let def = self.passing_def(name, accessed, receiver, setter, span)?;
        let actx = Ctx { owner: host, ..ctx };
        self.index(&[def], actx);
        let sym = self.syms.by_tree[&def];
        self.accessors.pending.entry(host).or_default().push((def, actx));
        if setter {
            self.complete(sym);
            let info = self.u.info(sym);
            let info = with_receiver_named(self.u, info);
            self.u.set_info(sym, info);
        }
        Some(sym)
    }
}
