// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/ConstraintHandling.scala
//! Bounds without the local symbols defined after a variable (dotty's `LevelAvoidMap`).

use super::levels::is_local_since;
use super::unify::meet;
use super::{At, Constraint};
use tasty::tags::PRIVATE;
use tasty::{ANY, Kind, SymbolId, Type, TypeId, TypeVars, Universe};

impl Constraint {
    /// Whether a local symbol escaped into the bound of a variable made before it.
    pub fn escaped(&self) -> bool {
        self.escaped
    }

    /// `t` at variance `v` without the local symbols defined since `at`: where covariant, a
    /// local class without members of its own replaced by its parents, a local value or method by
    /// its type, a class member of a local value selected on the value's type, a variable nested
    /// more deeply than `at` whose bounds refer to one by a new variable above it made `at` (dotty's
    /// `legalVar`); a type member of a local value by its alias, or by its bound where not
    /// invariant; an invariant argument by the wildcard of its avoided bounds where covariant.
    /// `None` if another such symbol remains.
    pub fn avoided(&mut self, u: &mut Universe, t: TypeId, at: At, v: i32) -> Option<TypeId> {
        if !self.refers_to_local(u, t, at, 4) {
            return Some(t);
        }
        match u.types().get(t).clone() {
            Type::Var(x) => match self.vars[x.0 as usize].instance {
                Some(i) => self.avoided(u, i, at, v),
                None if v > 0 => {
                    let above = self.fresh_at(u, at);
                    self.add_upper(u, x, above).then_some(above)
                }
                None if v == 0 => self.unified_at(u, x, at),
                None => None,
            },
            Type::Applied(tycon, _) if u.class_symbol(tycon).is_none() => {
                let dealiased = u.dealias(t);
                if dealiased == t { None } else { self.avoided(u, dealiased, at, v) }
            }
            Type::Sym(Some(pre), s) if !is_local_since(u, s, at.born) => match u.symbol(s).kind {
                Kind::Class if v > 0 => {
                    let widened = u.widen(pre);
                    let selected = u.intern_type(Type::Sym(Some(widened), s));
                    if widened == pre { None } else { self.avoided(u, selected, at, v) }
                }
                Kind::Type => {
                    let dealiased = u.dealias(t);
                    if dealiased != t {
                        return self.avoided(u, dealiased, at, v);
                    }
                    let info = u.member_info(pre, s);
                    match *u.types().get(info) {
                        Type::Bounds(_, hi) if v > 0 => self.avoided(u, hi, at, v),
                        Type::Bounds(lo, _) if v < 0 => self.avoided(u, lo, at, v),
                        _ => None,
                    }
                }
                Kind::TypeParam => {
                    let (lo, hi) = class_param_bounds(u, pre, s)?;
                    match v {
                        1 => self.avoided(u, hi, at, v),
                        -1 => self.avoided(u, lo, at, v),
                        _ => None,
                    }
                }
                _ if v > 0 => self.widened(u, t, at, v),
                _ => None,
            },
            Type::Sym(None, s) if u.symbol(s).kind == Kind::Type => {
                let info = u.info(s);
                match *u.types().get(info) {
                    Type::Alias(a) => self.avoided(u, a, at, v),
                    Type::Bounds(_, hi) if v > 0 => self.avoided(u, hi, at, v),
                    Type::Bounds(lo, _) if v < 0 => self.avoided(u, lo, at, v),
                    _ => None,
                }
            }
            Type::Sym(_, s) if v > 0 => match u.symbol(s).kind {
                Kind::Class if owned_by_term(u, s) && !declares_members(u, s) => {
                    let parents = u.parents(s).into_iter().fold(ANY, |acc, p| meet(u, acc, p));
                    self.avoided(u, parents, at, v)
                }
                Kind::Val | Kind::Param | Kind::Def => self.widened(u, t, at, v),
                _ => None,
            },
            Type::Applied(tycon, args) => {
                let class = u.class_symbol(tycon)?;
                let variances = u.variances(class);
                if variances.len() != args.len() || self.refers_to_local(u, tycon, at, 4) {
                    return None;
                }
                let mut out = Vec::with_capacity(args.len());
                for (&a, &pv) in args.iter().zip(&variances) {
                    if pv == 0 && self.is_open_var(u, a) {
                        out.push(self.avoided(u, a, at, 0)?);
                        continue;
                    }
                    if pv == 0 && self.refers_to_local(u, a, at, 4) {
                        let (bounds, wildcard) = match *u.types().get(a) {
                            Type::Bounds(lo, hi) => ((lo, hi), true),
                            _ => ((a, a), false),
                        };
                        let lo = self.avoided(u, bounds.0, at, -1)?;
                        let hi = self.avoided(u, bounds.1, at, 1)?;
                        match (wildcard, lo == hi) {
                            (false, true) => out.push(lo),
                            (false, false) if v <= 0 => return None,
                            _ => out.push(u.intern_type(Type::Bounds(lo, hi))),
                        }
                        continue;
                    }
                    out.push(self.avoided(u, a, at, v * i32::from(pv))?);
                }
                Some(u.intern_type(Type::Applied(tycon, out)))
            }
            Type::Refined(parent, name, info) => {
                let parent = self.avoided(u, parent, at, v)?;
                let info = match u.types().get(info).clone() {
                    _ if !self.refers_to_local(u, info, at, 4) => info,
                    Type::Alias(_) if v > 0 => return Some(parent),
                    Type::Bounds(lo, hi) if v > 0 => {
                        let (lo, hi) = (self.avoided(u, lo, at, -1)?, self.avoided(u, hi, at, 1)?);
                        u.intern_type(Type::Bounds(lo, hi))
                    }
                    _ => return None,
                };
                Some(u.intern_type(Type::Refined(parent, name, info)))
            }
            Type::And(a, b) => {
                let (a, b) = (self.avoided(u, a, at, v)?, self.avoided(u, b, at, v)?);
                Some(meet(u, a, b))
            }
            Type::Or(a, b, soft) => {
                let (a, b) = (self.avoided(u, a, at, v)?, self.avoided(u, b, at, v)?);
                Some(u.intern_type(Type::Or(a, b, soft)))
            }
            _ => None,
        }
    }

    /// Value reference `t` widened and dealiased, then `avoided`.
    fn widened(&mut self, u: &mut Universe, t: TypeId, at: At, v: i32) -> Option<TypeId> {
        let widened = u.widen(t);
        let widened = match *u.types().get(widened) {
            Type::ByName(r) => r,
            _ => widened,
        };
        let widened = u.dealias(widened);
        if widened == t { None } else { self.avoided(u, widened, at, v) }
    }
}

/// The bounds of class type parameter `p` selected on `pre`: those of the wildcard argument
/// `pre`'s type gives it, the argument itself from both sides otherwise.
fn class_param_bounds(u: &mut Universe, pre: TypeId, p: SymbolId) -> Option<(TypeId, TypeId)> {
    let selected = u.intern_type(Type::Sym(Some(pre), p));
    let arg = crate::typer::seen_from::class_param_arg(u, selected)?;
    match *u.types().get(arg) {
        Type::Bounds(lo, hi) => Some((lo, hi)),
        _ => Some((arg, arg)),
    }
}

fn owned_by_term(u: &Universe, s: SymbolId) -> bool {
    u.symbol(s).owner.is_some_and(|o| matches!(u.symbol(o).kind, Kind::Def | Kind::Val))
}

/// Whether class `c` declares a member other than its constructor and private ones.
fn declares_members(u: &Universe, c: SymbolId) -> bool {
    u.symbol(c).decls.iter().any(|&d| {
        let s = u.symbol(d);
        s.name != "<init>" && !s.flags.has(PRIVATE)
    })
}
