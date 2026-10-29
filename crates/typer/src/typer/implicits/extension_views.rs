// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Implicits.scala
//! Implicit values, and the objects and values of an implicit scope, as containers of extension
//! methods (dotty's `Candidate.Extension`): `qual.name` typed as `c.name(qual)`.

use super::super::{Pt, Typer};
use super::candidates::Candidate;
use crate::ctx::Ctx;
use crate::tpd::TNodeId;
use syntax::ast::{NodeId, TName, Tree};
use tasty::{Binding, Type};

impl Typer<'_> {
    /// Selection `id` of `name` from `qual` as `c.name(qual)` for implicit value `c`.
    pub fn implicit_extension_select(
        &mut self,
        id: NodeId,
        qual: TNodeId,
        c: Candidate,
        name: TName,
        pt: Pt,
        ctx: Ctx,
    ) -> TNodeId {
        let at = self.tpd.span(qual).start_pos();
        let tpe = self.ref_type(c.prefix, c.sym);
        let info = self.method_info(tpe).unwrap_or(tpe);
        let container = match self.u.types().get(info) {
            Type::Method { .. } | Type::Poly { .. } => {
                let text = self.text(name.name).to_owned();
                self.conditional_container(c, qual, &text, ctx)
            }
            _ => self.candidate_ref(c, at, ctx),
        };
        let container = match container {
            Ok(t) => t,
            Err(what) => return self.unsupported(id, what),
        };
        let qualifier = self.spliced(container);
        let fun = self.ast.node(Tree::Select { qualifier, name });
        self.ext_method_apply(id, fun, qual, pt, ctx)
    }

    /// The reference to candidate `c` if it is a value with one extension method `name` whose
    /// receiver `tree` fits (dotty's `hasExtMethod`).
    pub fn try_extension(
        &mut self,
        c: Candidate,
        tree: TNodeId,
        name: &str,
        ctx: Ctx,
    ) -> Result<TNodeId, &'static str> {
        if !self.offers_extension(c, name)? {
            return Err("not an implicit value");
        }
        let tpe = self.ref_type(c.prefix, c.sym);
        let info = self.method_info(tpe).unwrap_or(tpe);
        if matches!(self.u.types().get(info), Type::Method { .. } | Type::Poly { .. }) {
            return self.conditional_container(c, tree, name, ctx);
        }
        let at = self.tpd.span(tree).start_pos();
        let container = self.candidate_ref(c, at, ctx)?;
        let value = self.tpd.tpe(container);
        let methods = self.extension_members(value, name)?;
        if let [_, _, ..] = methods[..] {
            for m in methods {
                let (fits, clean, _) = self.speculatively(|t| t.receiver_fits(container, m, tree));
                if clean && fits.is_ok() {
                    return Ok(container);
                }
            }
            return Err("implicit of another type");
        }
        let [method] = methods[..] else { return Err("implicit of another type") };
        self.receiver_fits(container, method, tree).map(|()| container)
    }

    /// Implicit method `c` applied to type variables and to its implicit arguments, if its
    /// result has one extension method `name` whose receiver `tree` fits, the receiver
    /// constraining the variables before the arguments are searched (dotty's `typedImplicit` of
    /// an extension candidate).
    fn conditional_container(
        &mut self,
        c: Candidate,
        tree: TNodeId,
        name: &str,
        ctx: Ctx,
    ) -> Result<TNodeId, &'static str> {
        let at = self.tpd.span(tree).start_pos();
        let container = self.candidate_ref(c, at, ctx)?;
        let container = self.with_type_vars(container)?;
        let tpe = self.tpd.tpe(container);
        let info = self.method_info(tpe).unwrap_or(tpe);
        let value = self.strip_implicit(info);
        if matches!(self.u.types().get(value), Type::Method { .. } | Type::Poly { .. }) {
            return Err("extension method of an implicit method");
        }
        let by_name = self
            .u
            .param_clauses(info)
            .iter()
            .flat_map(|(_, ps)| ps.clone())
            .any(|p| matches!(self.u.types().get(p), Type::ByName(_)));
        if by_name {
            return Err("extension method of an implicit method with by-name parameters");
        }
        let from = self.tpd.tpe(tree);
        let fits = |t: &mut Self, method| {
            let selected = t.ref_type(Some(value), method);
            let method_info = t.method_info(selected).unwrap_or(selected);
            let receiver =
                t.receiver_type(method, method_info).ok_or("extension method of an implicit method")?;
            Ok::<bool, &'static str>(t.conforms(from, receiver))
        };
        for method in self.extension_members(value, name)? {
            let (fit, clean, _) = self.speculatively(|t| fits(t, method));
            if clean && fit? && fits(self, method)? {
                return self.with_implicit_args(container, (at, at), ctx, 1);
            }
        }
        Err("implicit of another type")
    }

    /// Whether extension method `method` of implicit value `container` takes `tree` as its
    /// receiver; `Err` saying why not.
    fn receiver_fits(
        &mut self,
        container: TNodeId,
        method: tasty::SymbolId,
        tree: TNodeId,
    ) -> Result<(), &'static str> {
        let at = self.tpd.span(tree).start_pos();
        let select = self.build().select(container, method, at);
        let select = self.with_type_vars(select)?;
        let tpe = self.tpd.tpe(select);
        let info = self.method_info(tpe).unwrap_or(tpe);
        let Some(Binding { params, implicit: false, .. }) = self.u.method(info) else {
            return Err("extension method with a using clause before its receiver");
        };
        let [(_, mut receiver)] = params[..] else { return Err("extension method of an implicit value") };
        if let Type::ByName(r) = *self.u.types().get(receiver) {
            receiver = r;
        }
        let from = self.tpd.tpe(tree);
        let from = self.zonk(from);
        let from = self.u.normalized(from);
        if self.conforms(from, receiver) {
            return Ok(());
        }
        if self.constant_of(tree).is_some() && self.widens_to(from, receiver) {
            return Err("extension method of an implicit value on a widened constant");
        }
        Err("implicit of another type")
    }
}
