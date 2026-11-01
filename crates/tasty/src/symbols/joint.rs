// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/Denotations.scala
//! The joint members of intersections: one symbol with the meet of both sides' types (dotty's
//! `JointRefDenotation`, `infoMeet`).

use super::{SymbolId, Universe};
use crate::subtype::NoVars;
use crate::types::{Binder, Type, TypeId};

/// How many steps finding the intersection a prefix selects a joint member on takes at most.
const MAX_STEPS: u32 = 64;

impl Universe {
    /// Records `info` as the type of member `m` of intersection `site`.
    pub fn set_joint_info(&mut self, site: TypeId, m: SymbolId, info: TypeId) {
        let entries = self.joint_infos.entry(m).or_default();
        if !entries.iter().any(|&(s, _)| s == site) {
            entries.push((site, info));
        }
    }

    /// The type of member `m` of intersection `site` that its sides give it: a joint type of a
    /// nested intersection, else `m`'s own.
    pub fn side_member_info(&mut self, site: TypeId, m: SymbolId) -> TypeId {
        let joint = match *self.types().get(site) {
            Type::And(a, b) => self.joint_info(a, m).or_else(|| self.joint_info(b, m)),
            _ => None,
        };
        joint.unwrap_or_else(|| self.own_member_info(site, m))
    }

    /// The joint type of member `m` selected on `prefix`, through its underlying types, the
    /// given self type of a `this`, bounds and both sides of intersections.
    pub fn joint_info(&mut self, prefix: TypeId, m: SymbolId) -> Option<TypeId> {
        if !self.joint_infos.contains_key(&m) {
            return None;
        }
        let mut t = prefix;
        for _ in 0..MAX_STEPS {
            if let Some(&(_, info)) = self.joint_infos[&m].iter().find(|&&(s, _)| s == t) {
                return Some(info);
            }
            t = match *self.types().get(t) {
                Type::And(a, b) => return self.joint_info(a, m).or_else(|| self.joint_info(b, m)),
                Type::This(c) => {
                    let c = self.class_symbol(c)?;
                    self.given_self_type(c)?;
                    self.self_type(c)
                }
                Type::Refined(p, ..) | Type::Annotated(p, _) | Type::Rec(_, p) | Type::Bounds(_, p) => p,
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
        match (self.types().get(a).clone(), self.types().get(b).clone()) {
            (
                Type::Method { binder, params, result, implicit, contextual },
                Type::Method { binder: b2, params: p2, result: r2, implicit: i2, .. },
            ) => {
                let r2 = self.with_binder(r2, (b2, &p2), binder);
                let theirs: Vec<TypeId> =
                    p2.iter().map(|&(_, p)| self.with_binder(p, (b2, &p2), binder)).collect();
                let matching = params.len() == p2.len()
                    && params
                        .iter()
                        .zip(&theirs)
                        .all(|(&(_, x), &y)| self.is_same_type_in(x, y, &mut NoVars));
                if !matching || implicit != i2 {
                    return None;
                }
                let result = self.info_meet(result, r2)?;
                let params = merged_names(&params, &p2, "x$")
                    .into_iter()
                    .zip(&params)
                    .map(|(n, &(_, t))| (n, t))
                    .collect();
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
    fn with_binder(
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
        match (self.types().get(a), self.types().get(b)) {
            (&Type::Bounds(l1, h1), &Type::Bounds(l2, h2)) if a != b => {
                let (lo, hi) = (self.lub(l1, l2), self.glb(h1, h2));
                self.intern_type(Type::Bounds(lo, hi))
            }
            _ => a,
        }
    }
}

/// The names of `a`'s parameters, a synthetic one where `b`'s differs (dotty's
/// `mergeParamNames`).
fn merged_names(a: &[(String, TypeId)], b: &[(String, TypeId)], synthetic: &str) -> Vec<String> {
    a.iter()
        .zip(b)
        .enumerate()
        .map(|(i, ((x, _), (y, _)))| if x == y { x.clone() } else { format!("{synthetic}{i}") })
        .collect()
}
