// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/Types.scala
//! Whether a type has class symbol `Nothing` (dotty's `hasClassSymbol(defn.NothingClass)`).

use super::refs::Meaning;
use super::{Cmp, MAX_DEPTH};
use crate::types::{Type, TypeId};

impl Cmp<'_> {
    /// Whether `t` has class symbol `Nothing`; `None` for a type this comparison does not
    /// decide it for.
    pub fn is_nothing(&mut self, t: TypeId) -> Option<bool> {
        self.walk.clear();
        self.nothing_at(t, 0)
    }

    /// Whether `t`, reached in `depth` steps that are not unions or intersections, has class
    /// symbol `Nothing`; kept for the walk.
    pub fn nothing_at(&mut self, t: TypeId, depth: u32) -> Option<bool> {
        if let Some((nothing, provisional)) = self.walk.nothing(t, depth) {
            self.provisional |= provisional;
            return nothing;
        }
        if depth >= MAX_DEPTH || !self.step() {
            self.provisional = true;
            return None;
        }
        let outer = std::mem::replace(&mut self.provisional, false);
        let nothing = self.own_nothing(t, depth);
        self.walk.set_nothing(t, depth, nothing, self.provisional);
        self.provisional |= outer;
        nothing
    }

    fn own_nothing(&mut self, t: TypeId, depth: u32) -> Option<bool> {
        let d = depth + 1;
        match *self.u.types().get(t) {
            Type::Constant(..) | Type::Str(_) | Type::This(_) | Type::Package(_) | Type::NoPrefix => {
                Some(false)
            }
            Type::Or(x, y, _) => match self.nothing_at(x, depth) {
                Some(false) => Some(false),
                nx => match (nx, self.nothing_at(y, depth)) {
                    (_, Some(false)) => Some(false),
                    (Some(true), Some(true)) => Some(true),
                    _ => None,
                },
            },
            Type::And(x, y) => match self.nothing_at(x, depth) {
                Some(true) => Some(true),
                nx => match (nx, self.nothing_at(y, depth)) {
                    (_, Some(true)) => Some(true),
                    (Some(false), Some(false)) => Some(false),
                    _ => None,
                },
            },
            Type::ByName(r) | Type::Refined(r, ..) | Type::Rec(_, r) | Type::Annotated(r, _) => {
                self.nothing_at(r, d)
            }
            Type::Var(v) => {
                self.provisional = true;
                let bound = self.vars.instance(v).unwrap_or(self.vars.bounds(v).1);
                self.nothing_at(bound, d)
            }
            Type::TypeRef(..) | Type::TermRef(..) | Type::Sym(..) | Type::Applied(..) => {
                self.provisional = true;
                self.named_is_nothing(t, d)
            }
            _ => None,
        }
    }

    fn named_is_nothing(&mut self, t: TypeId, d: u32) -> Option<bool> {
        let t = self.u.dealias(t);
        let nothing = self.u.builtins().nothing;
        match self.u.types().get(t).clone() {
            Type::Applied(tycon, args) => match self.named(tycon)?.meaning {
                Meaning::Class(c) => Some(c == nothing),
                Meaning::Abstract(_, hi) => {
                    let upper = self.apply(hi, &args);
                    self.nothing_at(upper, d)
                }
                _ => None,
            },
            Type::TypeRef(..) | Type::TermRef(..) | Type::Sym(..) => match self.named(t)?.meaning {
                Meaning::Class(c) => Some(c == nothing),
                Meaning::Alias(r) | Meaning::Abstract(_, r) => self.nothing_at(r, d),
                Meaning::Term(_) => {
                    let w = self.u.widen_once(t)?;
                    self.nothing_at(w, d)
                }
                _ => None,
            },
            _ => self.nothing_at(t, d),
        }
    }
}
