// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/Denotations.scala
//! The joint members of intersections: one symbol with the meet of both sides' types (dotty's
//! `JointRefDenotation`, `infoMeet`).

use super::{SymbolId, Universe};
use crate::subtype::NoVars;
use crate::types::{Binder, Type, TypeId};
use rustc_hash::FxHashMap;

/// How many steps finding the intersection a prefix selects a joint member on takes at most.
const MAX_STEPS: u32 = 64;

/// Per joint member of intersections, each intersection and the members of its two sides.
pub type JointMembers = FxHashMap<SymbolId, Vec<(TypeId, (SymbolId, SymbolId))>>;

impl Universe {
    /// Records `m` as the joint member of intersection `site` whose sides have members `sides`.
    pub fn set_joint_member(&mut self, site: TypeId, m: SymbolId, sides: (SymbolId, SymbolId)) {
        let entries = self.joint_members.entry(m).or_default();
        if !entries.iter().any(|&(s, _)| s == site) {
            entries.push((site, sides));
        }
    }

    /// The type of member `m` of intersection `site` that its sides give it: a joint type of a
    /// nested intersection, else `m`'s own.
    pub fn side_member_info(&mut self, site: TypeId, m: SymbolId) -> TypeId {
        self.side_info(site, site, m)
    }

    /// The joint type of member `m` selected on `prefix`: the meet of its sides' types seen from
    /// `prefix`.
    pub fn joint_info(&mut self, prefix: TypeId, m: SymbolId) -> Option<TypeId> {
        let site = self.joint_site(prefix, m)?;
        self.joint_at(prefix, site, m)
    }

    fn joint_at(&mut self, prefix: TypeId, site: TypeId, m: SymbolId) -> Option<TypeId> {
        let &(_, (a, b)) = self.joint_members[&m].iter().find(|&&(s, _)| s == site)?;
        let (ia, ib) = (self.side_info(prefix, site, a), self.side_info(prefix, site, b));
        self.info_meet(ia, ib)
    }

    /// The type of member `m` of a side of intersection `site`, seen from `prefix`.
    fn side_info(&mut self, prefix: TypeId, site: TypeId, m: SymbolId) -> TypeId {
        let nested = match *self.ty(site) {
            Type::And(a, b) => self.joint_site(a, m).or_else(|| self.joint_site(b, m)),
            _ => None,
        };
        nested.and_then(|n| self.joint_at(prefix, n, m)).unwrap_or_else(|| self.own_member_info(prefix, m))
    }

    /// The intersection with joint member `m` that `prefix` selects it on, through its
    /// underlying types, the given self type of a `this`, bounds and both sides of intersections.
    fn joint_site(&mut self, prefix: TypeId, m: SymbolId) -> Option<TypeId> {
        if !self.joint_members.contains_key(&m) {
            return None;
        }
        let mut t = prefix;
        for _ in 0..MAX_STEPS {
            if self.joint_members[&m].iter().any(|&(s, _)| s == t) {
                return Some(t);
            }
            t = match self.ty(t).clone() {
                Type::And(a, b) => return self.joint_site(a, m).or_else(|| self.joint_site(b, m)),
                Type::This(c) => {
                    let c = self.class_symbol(c)?;
                    self.given_self_type(c)?;
                    self.self_type(c)
                }
                Type::Refined(p, ..) | Type::Annotated(p, _) | Type::Rec(_, p) | Type::Bounds(_, p) => p,
                Type::Applied(tycon, args) if self.dealias(t) == t => {
                    let hi = self.abstract_upper(tycon)?;
                    let applied = self.intern_type(Type::Applied(hi, args));
                    self.beta_reduced(applied)
                }
                _ => match self.widen_once(t) {
                    Some(w) => w,
                    None => match self.dealias(t) {
                        d if d != t => d,
                        _ => self.abstract_upper(t)?,
                    },
                },
            };
        }
        None
    }

    /// The type of a member that both sides of an intersection give types `a` and `b`, `None` if
    /// they do not combine (dotty's `infoMeet`).
    pub fn info_meet(&mut self, a: TypeId, b: TypeId) -> Option<TypeId> {
        if a == b {
            return Some(a);
        }
        match (self.ty(a).clone(), self.ty(b).clone()) {
            (
                Type::Method { binder, params, result, implicit, contextual },
                Type::Method { binder: b2, params: p2, result: r2, implicit: i2, .. },
            ) => {
                let own = |u: &mut Self, t| u.param_syms_as_refs(t, &params, binder);
                let theirs = |u: &mut Self, t| {
                    let t = u.param_syms_as_refs(t, &p2, b2);
                    u.with_binder(t, (b2, &p2), binder)
                };
                let (result, r2) = (own(self, result), theirs(self, r2));
                let ours: Vec<TypeId> = params.iter().map(|&(_, p)| own(self, p)).collect();
                let theirs: Vec<TypeId> = p2.iter().map(|&(_, p)| theirs(self, p)).collect();
                let matching = params.len() == p2.len()
                    && ours.iter().zip(&theirs).all(|(&x, &y)| self.is_same_type_in(x, y, &mut NoVars));
                if !matching || implicit != i2 {
                    return None;
                }
                let result = self.info_meet(result, r2)?;
                let params = merged_names(&params, &p2, "x$").into_iter().zip(ours).collect();
                Some(self.intern_type(Type::Method { binder, params, result, implicit, contextual }))
            }
            (Type::Poly { binder, params, result }, Type::Poly { binder: b2, params: p2, result: r2 })
                if params.len() == p2.len() =>
            {
                let r2 = self.with_binder(r2, (b2, &p2), binder);
                let result = self.info_meet(result, r2)?;
                let names = merged_names(&params, &p2, "X");
                let mut merged = Vec::new();
                for ((n, &(_, x)), &(_, y)) in names.into_iter().zip(&params).zip(&p2) {
                    let y = self.with_binder(y, (b2, &p2), binder);
                    merged.push((n, self.bounds_meet(x, y)));
                }
                Some(self.intern_type(Type::Poly { binder, params: merged, result }))
            }
            (Type::Method { .. } | Type::Poly { .. }, _) | (_, Type::Method { .. } | Type::Poly { .. }) => {
                None
            }
            (Type::ByName(x), Type::ByName(y)) => {
                let r = self.glb(x, y);
                Some(self.intern_type(Type::ByName(r)))
            }
            (Type::ByName(x), _) => self.info_meet(x, b),
            (_, Type::ByName(y)) => Some(self.glb(a, y)),
            _ => Some(self.glb(a, b)),
        }
    }

    /// `t` with the parameters of `binder`, named as `params`, replaced by those of `to`.
    pub fn with_binder(
        &mut self,
        t: TypeId,
        (binder, params): (Binder, &[(String, TypeId)]),
        to: Binder,
    ) -> TypeId {
        let names: Vec<String> = params.iter().map(|(n, _)| n.clone()).collect();
        let refs: Vec<TypeId> =
            (0..names.len() as u32).map(|i| self.intern_type(Type::Param(to, i))).collect();
        self.subst(t, binder, &names, &refs)
    }

    /// The intersection of type parameter bounds `a` and `b`.
    fn bounds_meet(&mut self, a: TypeId, b: TypeId) -> TypeId {
        match (self.ty(a), self.ty(b)) {
            (&Type::Bounds(l1, h1), &Type::Bounds(l2, h2)) if a != b => {
                let (lo, hi) = (self.lub(l1, l2), self.glb(h1, h2));
                self.intern_type(Type::Bounds(lo, hi))
            }
            _ => a,
        }
    }
}

/// The names of `a`'s parameters, a synthetic one where `b`'s differs (dotty's `mergeParamNames`).
fn merged_names(a: &[(String, TypeId)], b: &[(String, TypeId)], synthetic: &str) -> Vec<String> {
    a.iter()
        .zip(b)
        .enumerate()
        .map(|(i, ((x, _), (y, _)))| if x == y { x.clone() } else { format!("{synthetic}{i}") })
        .collect()
}
