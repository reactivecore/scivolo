// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/ProtoTypes.scala
//! Conversions of a qualifier whose member is applied: those whose member applies to the
//! arguments and fits the expected result (dotty's `SelectionProto.isMatchedBy` with a
//! `FunProto` member prototype).

use super::super::super::{Pt, Typer};
use super::super::candidates::Candidate;
use super::super::divergence::THROUGH_CONVERSION;
use super::member_fits::{member_applies, member_fits};
use super::qualifier::is_plain_arg;
use super::{Goal, ViewProto, ViewSearch};
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
        let view_proto = t.applied_view_proto(proto, args.clone(), true);
        let member = ViewSearch::new(self.tree, Goal::Applied(name, &view_proto), self.ctx);
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

    /// Narrows ambiguous conversions `found` to those whose result's member `name` applies to
    /// the argument types of `proto`, as for `n % total` with `n: Long` and `total: BigInt`, where
    /// `long2float` and `long2double` both give a `%` that takes no `BigInt`; keeps `found` when
    /// that is unknown for a conversion.
    pub fn applying_to(
        &self,
        t: &mut Typer<'_>,
        found: Vec<Candidate>,
        name: &str,
        proto: &ViewProto,
    ) -> Vec<Candidate> {
        let args = &proto.parts[..proto.args];
        let reported = t.diags.items.len();
        let mut applying = Vec::new();
        for &c in &found {
            let (fits, clean, _) = t.speculatively(|t| {
                let q = self.try_conversion(t, c, false);
                q.is_ok_and(|q| member_applies(t, q, name, args, self.ctx))
            });
            t.retract_diags(reported);
            if !clean {
                return found;
            }
            if fits {
                applying.push(c);
            }
        }
        applying
    }

    /// Converts the tree to have a member `name` by the only conversion of the implicit scopes
    /// of argument types `args` that gives it one (the scope of dotty's `SelectionProto` of a
    /// `FunProto`).
    pub fn arg_scope_view(&self, t: &mut Typer<'_>, name: &str, args: &[TypeId]) -> Option<TNodeId> {
        let mut candidates = Vec::new();
        for &a in args {
            for c in t.implicit_scope(a).ok()? {
                if !candidates.contains(&c) {
                    candidates.push(c);
                }
            }
        }
        let mut fitting = Vec::new();
        let reported = t.diags.items.len();
        for c in candidates {
            let (fits, clean, _) = t.speculatively(|t| match self.try_conversion(t, c, false) {
                Ok(q) => Ok(member_applies(t, q, name, args, self.ctx)),
                Err(THROUGH_CONVERSION) => Err(THROUGH_CONVERSION),
                Err(_) => Ok(false),
            });
            t.retract_diags(reported);
            let fits = match fits {
                Ok(fits) => fits,
                Err(what) => {
                    t.unsupported_at(t.tpd.span(self.tree), what);
                    return None;
                }
            };
            if fits && clean {
                fitting.push(c);
            }
        }
        let [c] = fitting[..] else { return None };
        self.try_conversion(t, c, false).ok()
    }
}

/// The types of the arguments of `proto` if all are literals or paths and none is a method,
/// typed speculatively without diagnostics.
fn plain_arg_types(t: &mut Typer<'_>, proto: usize) -> Option<Vec<TypeId>> {
    let (args, arg_ctx) = (t.protos[proto].args.clone(), t.protos[proto].ctx);
    if t.protos[proto].typed.is_some()
        || !args.iter().all(|&a| is_plain_arg(t.ast, a) && !t.is_sequence_arg(a))
    {
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
