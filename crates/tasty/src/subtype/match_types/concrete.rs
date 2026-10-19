// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/MatchTypes.scala
use super::super::Cmp;
use crate::types::{Type, TypeId};
use crate::{Kind, NOTHING};

impl Cmp<'_> {
    /// Whether `t` determines the instances of captures uniquely: no upper bound of an abstract
    /// type followed (dotty's `MatchTypes.isConcrete`).
    pub fn is_concrete(&mut self, t: TypeId, depth: u32) -> bool {
        let Some(depth) = depth.checked_sub(1) else { return false };
        let t = self.normal(t);
        match self.u.types().get(t).clone() {
            Type::Sym(..) | Type::TypeRef(..) | Type::TermRef(..) => {
                let Some(s) = self.named(t).and_then(|n| n.sym) else { return false };
                match self.u.symbol(s).kind {
                    Kind::Class => true,
                    Kind::Type => match self.u.dealias(t) {
                        d if d != t => self.is_concrete(d, depth),
                        _ => false,
                    },
                    Kind::TypeParam => false,
                    Kind::Param => false,
                    _ => match self.u.widen_once(t) {
                        Some(w) => self.is_concrete(w, depth),
                        None => false,
                    },
                }
            }
            Type::Applied(tycon, args) => {
                self.is_concrete(tycon, depth)
                    && match self.applied_super_type(tycon, &args) {
                        Some(sup) => self.is_concrete(sup, depth),
                        None => true,
                    }
            }
            Type::Lambda { .. } => true,
            Type::Param(..) | Type::Match { .. } | Type::Var(_) => false,
            Type::And(a, b) | Type::Or(a, b, _) => self.is_concrete(a, depth) && self.is_concrete(b, depth),
            Type::This(_) | Type::Constant(..) | Type::Str(_) | Type::Skolem(..) => {
                match self.u.widen_once(t) {
                    Some(w) => self.is_concrete(w, depth),
                    None => false,
                }
            }
            Type::Annotated(p, _) | Type::Refined(p, ..) | Type::Rec(_, p) | Type::ByName(p) => {
                self.is_concrete(p, depth)
            }
            _ => t == NOTHING,
        }
    }
}
