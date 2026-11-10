// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Implicits.scala
//! Implicit values, and the objects and values of an implicit scope, as containers of extension
//! methods (dotty's `Candidate.Extension`): `qual.name` typed as `c.name(qual)`.

use super::super::super::{Pt, Typer};
use super::super::candidates::{Candidate, candidate_ref};
use super::fallback::receiver_type;
use super::view_kinds::{extension_members, offers_extension};
use super::{Goal, ViewSearch};
use crate::ctx::Ctx;
use crate::tpd::TNodeId;
use syntax::ast::{NodeId, TName, Tree};
use tasty::{Binding, SymbolId, Type};

impl Typer<'_> {
    /// Types selection `id` of `name` from `qual` as `c.name(qual)`, the extension method of
    /// implicit value `c` a view search found, as `"ab".twice` becomes `ops.twice("ab")`.
    pub fn implicit_extension_select(
        &mut self,
        id: NodeId,
        qual: TNodeId,
        c: Candidate,
        name: TName,
        pt: Pt,
        ctx: Ctx,
    ) -> TNodeId {
        let text = self.text(name.name).to_owned();
        let container = match ViewSearch::new(qual, Goal::Member(&text), ctx).container(self, c, &text) {
            Ok(t) => t,
            Err(what) => return self.unsupported(id, what),
        };
        let qualifier = self.spliced(container);
        let fun = self.ast.node(Tree::Select { qualifier, name });
        self.ext_method_apply(id, fun, qual, pt, ctx)
    }
}

impl ViewSearch<'_> {
    /// Gives the container of the extension method `name` of implicit value `c` for the tree as
    /// receiver: a reference to `c`, an implicit method applied as for `try_extension`.
    fn container(&self, t: &mut Typer<'_>, c: Candidate, name: &str) -> Result<TNodeId, &'static str> {
        let at = t.tpd.span(self.tree).start_pos();
        let tpe = t.ref_type(c.prefix, c.sym);
        let info = t.method_info(tpe).unwrap_or(tpe);
        match *t.u.ty(info) {
            Type::Method { .. } | Type::Poly { .. } => self.conditional_container(t, c, name),
            _ => candidate_ref(t, c, at, self.ctx),
        }
    }

    /// Gives the reference to candidate `c` if it is a value with one extension method `name`
    /// that takes the tree as its receiver, or with several of which one does; an implicit
    /// method applied to type variables and implicit arguments first (dotty's `hasExtMethod`).
    pub fn try_extension(
        &self,
        t: &mut Typer<'_>,
        c: Candidate,
        name: &str,
    ) -> Result<TNodeId, &'static str> {
        if !offers_extension(t, c, name)? {
            return Err("not an implicit value");
        }
        let tpe = t.ref_type(c.prefix, c.sym);
        let info = t.method_info(tpe).unwrap_or(tpe);
        if matches!(t.u.ty(info), Type::Method { .. } | Type::Poly { .. }) {
            return self.conditional_container(t, c, name);
        }
        let at = t.tpd.span(self.tree).start_pos();
        let container = candidate_ref(t, c, at, self.ctx)?;
        let value = t.tpd.tpe(container);
        let methods = extension_members(t, value, name)?;
        if let [_, _, ..] = methods[..] {
            for m in methods {
                let (fits, clean, _) = t.speculatively(|t| self.receiver_fits(t, container, m));
                if clean && fits.is_ok() {
                    return Ok(container);
                }
            }
            return Err("implicit of another type");
        }
        let [method] = methods[..] else { return Err("implicit of another type") };
        self.receiver_fits(t, container, method).map(|()| container)
    }

    /// Applies implicit method `c` to type variables and to its implicit arguments, if its result
    /// has one extension method `name` whose receiver the tree fits, the receiver constraining the
    /// variables before the arguments are searched (dotty's `typedImplicit` of an extension
    /// candidate).
    fn conditional_container(
        &self,
        t: &mut Typer<'_>,
        c: Candidate,
        name: &str,
    ) -> Result<TNodeId, &'static str> {
        let at = t.tpd.span(self.tree).start_pos();
        let container = candidate_ref(t, c, at, self.ctx)?;
        let container = t.with_type_vars(container)?;
        let tpe = t.tpd.tpe(container);
        let info = t.method_info(tpe).unwrap_or(tpe);
        let value = t.strip_implicit(info);
        if matches!(t.u.ty(value), Type::Method { .. } | Type::Poly { .. }) {
            return Err("extension method of an implicit method");
        }
        let by_name =
            t.u.param_clauses(info).iter().flat_map(|(_, ps)| ps.clone()).any(|p| t.u.by_name(p).is_some());
        if by_name {
            return Err("extension method of an implicit method with by-name parameters");
        }
        let from = t.tpd.tpe(self.tree);
        let fits = |t: &mut Typer<'_>, method| {
            let selected = t.ref_type(Some(value), method);
            let method_info = t.method_info(selected).unwrap_or(selected);
            let receiver =
                receiver_type(t, method, method_info).ok_or("extension method of an implicit method")?;
            Ok::<bool, &'static str>(t.conforms(from, receiver))
        };
        for method in extension_members(t, value, name)? {
            let (fit, clean, _) = t.speculatively(|t| fits(t, method));
            if clean && fit? && fits(t, method)? {
                return t.with_implicit_args(container, (at, at), self.ctx, 1);
            }
        }
        Err("implicit of another type")
    }

    /// Checks that extension method `method` of implicit value `container` takes the tree as its
    /// receiver; `Err` says why not.
    fn receiver_fits(
        &self,
        t: &mut Typer<'_>,
        container: TNodeId,
        method: SymbolId,
    ) -> Result<(), &'static str> {
        let at = t.tpd.span(self.tree).start_pos();
        let select = t.build().select(container, method, at);
        let mut select = t.with_type_vars(select)?;
        let mut info = t.method_info(t.tpd.tpe(select)).unwrap_or(t.tpd.tpe(select));
        if t.u.poly(info).is_some() {
            select = t.with_type_vars(select)?;
            info = t.method_info(t.tpd.tpe(select)).unwrap_or(t.tpd.tpe(select));
        }
        let Some(Binding { params, implicit: false, .. }) = t.u.method(info) else {
            return Err("extension method with a using clause before its receiver");
        };
        let [(_, mut receiver)] = params[..] else { return Err("extension method of an implicit value") };
        if let Some(r) = t.u.by_name(receiver) {
            receiver = r;
        }
        let from = t.tpd.tpe(self.tree);
        let from = t.zonk(from);
        let from = t.u.normalized(from);
        if t.conforms(from, receiver) {
            return Ok(());
        }
        if t.constant_of(self.tree).is_some() && t.types().widens_to(from, receiver) {
            return Err("extension method of an implicit value on a widened constant");
        }
        Err("implicit of another type")
    }
}
