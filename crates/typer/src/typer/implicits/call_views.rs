// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/ProtoTypes.scala
//! Conversions of a qualifier whose member is applied: those whose member applies to the
//! arguments and fits the expected result (dotty's `SelectionProto.isMatchedBy` with a
//! `FunProto` member prototype).

use super::super::{Pt, Typer};
use super::candidates::Candidate;
use super::conversions::Goal;
use crate::ctx::Ctx;
use crate::tpd::TNodeId;
use tasty::{Type, TypeId};

impl<'a> Goal<'a> {
    /// The goal of a conversion of the qualifier of a selection of `name` with expected type `pt`.
    pub fn of_select(name: &'a str, pt: Pt) -> Goal<'a> {
        match pt {
            Pt::Fun(proto) => Goal::Call(name, proto),
            Pt::Type(t) => Goal::Typed(name, t),
            _ => Goal::Member(name),
        }
    }

    /// The member the converted tree must have.
    pub fn member(self) -> Option<&'a str> {
        match self {
            Goal::Member(name) | Goal::Call(name, _) | Goal::Typed(name, _) => Some(name),
            Goal::Conform(_) => None,
        }
    }
}

impl Typer<'_> {
    /// The conversions of `found` whose result's member `name` applies to the arguments of
    /// `proto` and fits its expected result; `found` if the arguments are not literals or paths,
    /// or no conversion is left.
    pub fn applying(
        &mut self,
        found: Vec<Candidate>,
        tree: TNodeId,
        (name, proto): (&str, usize),
        ctx: Ctx,
    ) -> Vec<Candidate> {
        let Some(args) = self.plain_arg_types(proto) else { return found };
        let result = match self.protos[proto].pt {
            Pt::Type(t) => Some(t),
            _ => None,
        };
        let reported = self.diags.items.len();
        let mut applying = Vec::new();
        for &c in &found {
            let (fits, clean, _) = self.speculatively(|t| {
                let Ok(q) = t.try_conversion(c, (tree, false), Goal::Member(name), ctx) else { return false };
                t.member_fits(q, name, &args, result, ctx)
            });
            self.retract_diags(reported);
            if fits && clean {
                applying.push(c);
            }
        }
        if applying.is_empty() { found } else { applying }
    }

    /// The types of the arguments of `proto` if all are literals or paths and none is a method,
    /// typed speculatively without diagnostics.
    fn plain_arg_types(&mut self, proto: usize) -> Option<Vec<TypeId>> {
        let (args, arg_ctx) = (self.protos[proto].args.clone(), self.protos[proto].ctx);
        if self.protos[proto].typed.is_some() || !args.iter().all(|&a| self.is_plain_arg(a)) {
            return None;
        }
        let reported = self.diags.items.len();
        let (types, _, _) = self.speculatively(|t| {
            let mut types = Vec::new();
            for &a in &args {
                let typed = t.typed(a, Pt::Any, arg_ctx);
                let tpe = t.zonk(t.tpd.tpe(typed));
                let info = t.method_info(tpe).unwrap_or(tpe);
                if matches!(t.u.types().get(info), Type::Method { .. } | Type::Poly { .. }) {
                    return None;
                }
                types.push(tpe);
            }
            Some(types)
        });
        let diagnosed = self.diags.items.len() > reported;
        self.retract_diags(reported);
        types.filter(|_| !diagnosed)
    }

    /// Whether a member `name` of `q` surely applies to arguments of types `args`, its result
    /// conforming to `pt` if it is a method of one clause.
    fn member_fits(&mut self, q: TNodeId, name: &str, args: &[TypeId], pt: Option<TypeId>, ctx: Ctx) -> bool {
        let tpe = self.tpd.tpe(q);
        for m in self.type_members(tpe, name) {
            if self.applicable(m, Some(tpe), args, true, ctx) != Some(true) {
                continue;
            }
            let t = self.ref_type(Some(tpe), m);
            let info = self.method_info(t).unwrap_or(t);
            let Some(pt) = pt else { return true };
            match self.u.types().get(info).clone() {
                Type::Method { result, .. } if !matches!(self.u.types().get(result), Type::Method { .. }) => {
                    if self.conforms(result, pt) {
                        return true;
                    }
                }
                _ => return true,
            }
        }
        false
    }
}
