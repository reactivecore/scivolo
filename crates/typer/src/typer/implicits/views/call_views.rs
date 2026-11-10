// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/ProtoTypes.scala
//! Conversions of a qualifier whose member is applied: those whose member applies to the
//! arguments and fits the expected result (dotty's `SelectionProto.isMatchedBy` with a
//! `FunProto` member prototype).

use super::super::super::{Pt, Typer};
use super::super::candidates::Candidate;
use super::qualifier::is_plain_arg;
use super::{Goal, ViewSearch};
use crate::ctx::Ctx;
use crate::tpd::TNodeId;
use tasty::{Type, TypeId};

impl ViewSearch<'_> {
    /// Narrows ambiguous conversions `found` to those whose result's member `name` applies to the
    /// arguments of call `proto` and fits its expected result, as for `x.plus(1)` when two
    /// conversions give a `plus` (dotty's `deepenProto` after an ambiguous search); keeps `found`
    /// when the arguments are not literals or paths, or when that is unknown for a conversion.
    pub fn applying(
        &self,
        t: &mut Typer<'_>,
        found: Vec<Candidate>,
        name: &str,
        proto: usize,
    ) -> Vec<Candidate> {
        let Some(args) = plain_arg_types(t, proto) else { return found };
        let result = match t.protos[proto].pt {
            Pt::Type(pt) => Some(pt),
            _ => None,
        };
        let member = ViewSearch::new(self.tree, Goal::Member(name), self.ctx);
        let reported = t.diags.items.len();
        let mut applying = Vec::new();
        for &c in &found {
            let (fits, clean, _) = t.speculatively(|t| {
                let q = member.try_conversion(t, c, false).ok()?;
                member_fits(t, q, name, &args, result, self.ctx)
            });
            t.retract_diags(reported);
            match fits {
                None => return found,
                Some(true) if clean => applying.push(c),
                Some(_) => {}
            }
        }
        applying
    }
}

/// The types of the arguments of `proto` if all are literals or paths and none is a method,
/// typed speculatively without diagnostics.
fn plain_arg_types(t: &mut Typer<'_>, proto: usize) -> Option<Vec<TypeId>> {
    let (args, arg_ctx) = (t.protos[proto].args.clone(), t.protos[proto].ctx);
    if t.protos[proto].typed.is_some() || !args.iter().all(|&a| is_plain_arg(t.ast, a)) {
        return None;
    }
    let reported = t.diags.items.len();
    let (types, _, _) = t.speculatively(|t| {
        let mut types = Vec::new();
        for &a in &args {
            let typed = t.typed(a, Pt::Any, arg_ctx);
            let tpe = t.zonk(t.tpd.tpe(typed));
            let info = t.method_info(tpe).unwrap_or(tpe);
            if matches!(t.u.ty(info), Type::Method { .. } | Type::Poly { .. }) {
                return None;
            }
            types.push(tpe);
        }
        Some(types)
    });
    let diagnosed = t.diags.items.len() > reported;
    t.retract_diags(reported);
    types.filter(|_| !diagnosed)
}

/// Tells whether a member `name` of `q` applies to arguments of types `args`, its result
/// conforming to `pt` if it is a method of one clause; `None` if that is unknown.
fn member_fits(
    t: &mut Typer<'_>,
    q: TNodeId,
    name: &str,
    args: &[TypeId],
    pt: Option<TypeId>,
    ctx: Ctx,
) -> Option<bool> {
    let tpe = t.tpd.tpe(q);
    let mut known = true;
    for m in t.type_members(tpe, name) {
        match t.applicable(m, Some(tpe), args, true, ctx) {
            Some(true) => {}
            Some(false) => continue,
            None => {
                known = false;
                continue;
            }
        }
        let r = t.ref_type(Some(tpe), m);
        let info = t.method_info(r).unwrap_or(r);
        let Some(pt) = pt else { return Some(true) };
        match t.u.method(info).map(|m| m.result) {
            Some(result) if t.u.method(result).is_none() => {
                if t.conforms(result, pt) {
                    return Some(true);
                }
            }
            _ => return Some(true),
        }
    }
    known.then_some(false)
}
