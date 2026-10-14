//! Implicit values, and the objects and values of an implicit scope, as containers of extension
//! methods (dotty's `Candidate.Extension`): `qual.name` typed as `c.name(qual)`.

use super::super::{Pt, Typer};
use super::candidates::Candidate;
use crate::ctx::Ctx;
use crate::tpd::TNodeId;
use syntax::ast::{NodeId, TName, Tree};
use tasty::Type;

impl Typer<'_> {
    /// The one candidate of `group` with an extension method `name` that takes `tree`, if any;
    /// `Err` if there are several, or if a conversion of the same group fits too.
    pub fn extension_container(
        &mut self,
        group: &[Candidate],
        tree: TNodeId,
        name: &str,
        converts: bool,
        ctx: Ctx,
    ) -> Result<Option<Candidate>, &'static str> {
        let found = self.fitting(group, |t, c| t.try_extension(c, tree, name, ctx))?;
        match found[..] {
            [] => Ok(None),
            _ if converts => Err("implicit conversion beside an extension method of an implicit value"),
            [c] => Ok(Some(c)),
            _ => Err("ambiguous extension methods of implicit values"),
        }
    }

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
        let container = match self.candidate_ref(c, at, ctx) {
            Ok(t) => t,
            Err(what) => return self.unsupported(id, what),
        };
        let qualifier = self.spliced(container);
        let fun = self.ast.node(Tree::Select { qualifier, name });
        self.ext_method_apply(id, fun, qual, pt, ctx)
    }

    /// The reference to candidate `c` if it is a value with one extension method `name` whose
    /// receiver `tree` fits.
    fn try_extension(
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
            return Err("extension method of an implicit method");
        }
        let at = self.tpd.span(tree).start_pos();
        let container = self.candidate_ref(c, at, ctx)?;
        let value = self.tpd.tpe(container);
        let methods = self.extension_members(value, name)?;
        let [method] = methods[..] else { return Err("overloaded extension method of an implicit value") };
        let select = self.build().select(container, method, at);
        let select = self.with_type_vars(select)?;
        let tpe = self.tpd.tpe(select);
        let info = self.method_info(tpe).unwrap_or(tpe);
        let Type::Method { params, implicit: false, .. } = self.u.types().get(info).clone() else {
            return Err("extension method with a using clause before its receiver");
        };
        let [(_, mut receiver)] = params[..] else { return Err("extension method of an implicit value") };
        if let Type::ByName(r) = *self.u.types().get(receiver) {
            receiver = r;
        }
        let from = self.tpd.tpe(tree);
        if self.conforms(from, receiver) {
            return Ok(container);
        }
        if self.constant_of(tree).is_some() && self.widens_to(from, receiver) {
            return Err("extension method of an implicit value on a widened constant");
        }
        Err("implicit of another type")
    }
}
