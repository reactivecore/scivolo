// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Applications.scala
//! Whether one implicit candidate is as good as another by their types (dotty's `isAsGood` and
//! `isAsGoodValueType` with `preferGeneral`).

use super::super::Typer;
use super::candidates::Candidate;
use crate::ctx::Ctx;
use tasty::tags::{CONTRAVARIANT, COVARIANT, EXTENSION, IMPLICIT};
use tasty::{Type, TypeId};

impl Typer<'_> {
    /// Whether `a` of type `t1` is as good as `b` of type `t2` in `ctx`, the constraint
    /// unchanged; `None` if undecided.
    pub fn is_as_good(
        &mut self,
        (a, t1): (Candidate, TypeId),
        (b, t2): (Candidate, TypeId),
        ctx: Ctx,
    ) -> Option<bool> {
        let before = self.constraint.attempt();
        let result = self.as_good_in((a, t1), (b, t2), ctx);
        self.constraint.abandon(before);
        result
    }

    fn as_good_in(
        &mut self,
        (a, t1): (Candidate, TypeId),
        (b, t2): (Candidate, TypeId),
        ctx: Ctx,
    ) -> Option<bool> {
        match self.u.types().get(t1).clone() {
            Type::Method { params, .. } => {
                let lambda = matches!(
                    self.u.types().get(t2),
                    Type::Method { .. } | Type::Poly { .. } | Type::Lambda { .. }
                );
                if params.is_empty() && lambda {
                    return Some(true);
                }
                let extension = |t: &Self, c: Candidate| t.u.symbol(c.sym).flags.has(EXTENSION);
                if extension(self, a) && !extension(self, b) {
                    return Some(false);
                }
                let args: Vec<TypeId> = params.iter().map(|&(_, p)| p).collect();
                if args.last().is_some_and(|&p| self.repeated_elem(p).is_some()) {
                    return None;
                }
                let full_b = self.ref_type(b.prefix, b.sym);
                let full_b = self.method_info(full_b).unwrap_or(full_b);
                let full_b = self.poly_instance(full_b, b.sym, false);
                if !matches!(self.u.types().get(full_b), Type::Method { .. }) {
                    return Some(false);
                }
                self.applicable(b.sym, b.prefix, &args, false, ctx)
            }
            Type::Poly { .. } => {
                let t1 = self.poly_instance(t1, a.sym, true);
                self.as_good_in((a, t1), (b, t2), ctx)
            }
            _ => match self.u.types().get(t2).clone() {
                Type::Method { .. } => Some(true),
                Type::Poly { result, .. } if matches!(self.u.types().get(result), Type::Method { .. }) => {
                    Some(true)
                }
                Type::Poly { .. } => {
                    let t2 = self.poly_instance(t2, b.sym, false);
                    self.as_good_value(t1, t2, a)
                }
                _ => self.as_good_value(t1, t2, a),
            },
        }
    }

    /// dotty's `isAsGoodValueType`: an old-style implicit `a` by subtyping with contravariant
    /// arguments flipped, a given by the other way round.
    fn as_good_value(&mut self, t1: TypeId, t2: TypeId, a: Candidate) -> Option<bool> {
        let (p1, p2) = (self.prepared(t1), self.prepared(t2));
        if !self.u.symbol(a.sym).flags.has(IMPLICIT) {
            return Some(self.relaxed_conforms(p2, p1));
        }
        let (f1, f2) = (self.flipped(p1, 1)?, self.flipped(p2, 1)?);
        Some(self.relaxed_conforms(f1, f2))
    }

    /// `a <: b`, or numeric widening between value classes (dotty's `relaxed_<:<`).
    pub fn relaxed_conforms(&mut self, a: TypeId, b: TypeId) -> bool {
        self.conforms(a, b) || matches!(self.u.types().get(b), Type::Sym(..)) && self.widens_to(a, b)
    }

    /// `t` at variance `v` with each argument of a contravariant parameter in a covariant
    /// position replaced by a function from it to `Unit` (dotty's `flip`); `None` where the
    /// variance of a type constructor's parameters is unknown.
    fn flipped(&mut self, t: TypeId, v: i32) -> Option<TypeId> {
        if v == 0 {
            return Some(t);
        }
        match self.u.types().get(t).clone() {
            Type::Applied(tycon, args) => {
                let class = match self.u.types().get(tycon).clone() {
                    Type::Sym(_, s) => Some(s),
                    Type::TypeRef(p, n) => self.u.resolve(p, &n, true),
                    _ => None,
                };
                let class = class.filter(|&c| self.u.is_class(c))?;
                let params = self.u.type_params(class);
                if params.len() != args.len() {
                    return None;
                }
                let mut out = Vec::new();
                for (&arg, &p) in args.iter().zip(&params) {
                    let flags = &self.u.symbol(p).flags;
                    let pv = if flags.has(CONTRAVARIANT) { -1 } else { i32::from(flags.has(COVARIANT)) };
                    out.push(if v > 0 && pv < 0 {
                        let arg = self.flipped(arg, 1)?;
                        let unit = crate::defn::type_ref(self.u, self.defn.unit);
                        let function = self.function_class(1)?;
                        self.u.intern_type(Type::Applied(function, vec![arg, unit]))
                    } else {
                        self.flipped(arg, v * pv)?
                    });
                }
                Some(self.u.intern_type(Type::Applied(tycon, out)))
            }
            Type::And(x, y) => {
                let (x, y) = (self.flipped(x, v)?, self.flipped(y, v)?);
                Some(self.u.intern_type(Type::And(x, y)))
            }
            Type::Or(x, y, soft) => {
                let (x, y) = (self.flipped(x, v)?, self.flipped(y, v)?);
                Some(self.u.intern_type(Type::Or(x, y, soft)))
            }
            Type::Annotated(x, annot) => {
                let x = self.flipped(x, v)?;
                Some(self.u.intern_type(Type::Annotated(x, annot)))
            }
            _ => {
                let mut applied = false;
                self.u.map_type(t, &mut |u, part| {
                    applied |= matches!(u.types().get(part), Type::Applied(..));
                    None
                });
                (!applied).then_some(t)
            }
        }
    }
}
