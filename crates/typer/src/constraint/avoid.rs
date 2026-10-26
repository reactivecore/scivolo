// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/ConstraintHandling.scala
//! Bounds without the local symbols defined after a variable (dotty's `LevelAvoidMap`).

use super::Constraint;
use super::unify::meet;
use tasty::tags::PRIVATE;
use tasty::{ANY, Kind, SymbolId, Type, TypeId, TypeVars, Universe};

impl Constraint {
    /// `t` at variance `v` without the local symbols defined from `born` on: where covariant, a
    /// local class without members of its own replaced by its parents, a local value by its
    /// type, a class member of a local value selected on the value's type, a variable made after
    /// `born` whose bounds refer to one by a new variable above it made at `born` (dotty's
    /// `legalVar`); a type member of a local value by its alias, or by its bound where not
    /// invariant. `None` if another such symbol remains.
    pub fn avoided(&mut self, u: &mut Universe, t: TypeId, born: SymbolId, v: i32) -> Option<TypeId> {
        if !self.refers_to_local(u, t, born, 4) {
            return Some(t);
        }
        match u.types().get(t).clone() {
            Type::Var(x) => match self.vars[x.0 as usize].instance {
                Some(i) => self.avoided(u, i, born, v),
                None if v > 0 => {
                    let above = self.fresh_at(u, born);
                    self.add_upper(u, x, above).then_some(above)
                }
                None => None,
            },
            Type::Applied(tycon, _) if u.class_symbol(tycon).is_none() => {
                let dealiased = u.dealias(t);
                if dealiased == t { None } else { self.avoided(u, dealiased, born, v) }
            }
            Type::Sym(Some(pre), s) if !is_local_since(u, s, born) => match u.symbol(s).kind {
                Kind::Class if v > 0 => {
                    let widened = u.widen(pre);
                    let selected = u.intern_type(Type::Sym(Some(widened), s));
                    if widened == pre { None } else { self.avoided(u, selected, born, v) }
                }
                Kind::Type => {
                    let dealiased = u.dealias(t);
                    if dealiased != t {
                        return self.avoided(u, dealiased, born, v);
                    }
                    let info = u.member_info(pre, s);
                    match *u.types().get(info) {
                        Type::Bounds(_, hi) if v > 0 => self.avoided(u, hi, born, v),
                        Type::Bounds(lo, _) if v < 0 => self.avoided(u, lo, born, v),
                        _ => None,
                    }
                }
                Kind::TypeParam => {
                    let (lo, hi) = class_param_bounds(u, pre, s)?;
                    match v {
                        1 => self.avoided(u, hi, born, v),
                        -1 => self.avoided(u, lo, born, v),
                        _ => None,
                    }
                }
                _ if v > 0 => self.widened(u, t, born, v),
                _ => None,
            },
            Type::Sym(_, s) if v > 0 => match u.symbol(s).kind {
                Kind::Class if owned_by_term(u, s) && !declares_members(u, s) => {
                    let parents = u.parents(s).into_iter().fold(ANY, |acc, p| meet(u, acc, p));
                    self.avoided(u, parents, born, v)
                }
                Kind::Val | Kind::Param => self.widened(u, t, born, v),
                _ => None,
            },
            Type::Applied(tycon, args) => {
                let class = u.class_symbol(tycon)?;
                let variances = u.variances(class);
                if variances.len() != args.len() || self.refers_to_local(u, tycon, born, 4) {
                    return None;
                }
                let mut out = Vec::with_capacity(args.len());
                for (&a, &pv) in args.iter().zip(&variances) {
                    if pv == 0 && self.refers_to_local(u, a, born, 4) {
                        let bounds = match *u.types().get(a) {
                            Type::Bounds(lo, hi) => (lo, hi),
                            _ => (a, a),
                        };
                        let lo = self.avoided(u, bounds.0, born, -1).filter(|_| v != 0)?;
                        let hi = self.avoided(u, bounds.1, born, 1)?;
                        out.push(u.intern_type(Type::Bounds(lo, hi)));
                        continue;
                    }
                    out.push(self.avoided(u, a, born, v * i32::from(pv))?);
                }
                Some(u.intern_type(Type::Applied(tycon, out)))
            }
            Type::Refined(parent, name, info) => {
                let parent = self.avoided(u, parent, born, v)?;
                let info = match u.types().get(info).clone() {
                    _ if !self.refers_to_local(u, info, born, 4) => info,
                    Type::Alias(_) if v > 0 => return Some(parent),
                    Type::Bounds(lo, hi) if v > 0 => {
                        let (lo, hi) = (self.avoided(u, lo, born, -1)?, self.avoided(u, hi, born, 1)?);
                        u.intern_type(Type::Bounds(lo, hi))
                    }
                    _ => return None,
                };
                Some(u.intern_type(Type::Refined(parent, name, info)))
            }
            Type::And(a, b) => {
                let (a, b) = (self.avoided(u, a, born, v)?, self.avoided(u, b, born, v)?);
                Some(meet(u, a, b))
            }
            Type::Or(a, b, soft) => {
                let (a, b) = (self.avoided(u, a, born, v)?, self.avoided(u, b, born, v)?);
                Some(u.intern_type(Type::Or(a, b, soft)))
            }
            _ => None,
        }
    }

    /// Value reference `t` widened and dealiased, then `avoided`.
    fn widened(&mut self, u: &mut Universe, t: TypeId, born: SymbolId, v: i32) -> Option<TypeId> {
        let widened = u.widen(t);
        let widened = u.dealias(widened);
        if widened == t { None } else { self.avoided(u, widened, born, v) }
    }

    /// Whether `t` refers to a local symbol defined from `born` on, or has an uninstantiated
    /// variable made after `born` whose bounds do, `depth` variables deep.
    pub fn refers_to_local(&self, u: &mut Universe, t: TypeId, born: SymbolId, depth: u32) -> bool {
        let mut syms = Vec::new();
        let mut vars = Vec::new();
        u.map_type(t, &mut |u, part| {
            match *u.types().get(part) {
                Type::Sym(_, s) => syms.push(s),
                Type::Var(x) => vars.push(x),
                _ => {}
            }
            None
        });
        syms.into_iter().any(|s| is_local_since(u, s, born))
            || vars.into_iter().any(|x| {
                let e = self.vars[x.0 as usize];
                let bounds = |c: &Self, u: &mut Universe, b| c.refers_to_local(u, b, born, depth - 1);
                match e.instance {
                    Some(i) => depth == 0 || bounds(self, u, i),
                    None if e.born.0 <= born.0 => false,
                    None => depth == 0 || bounds(self, u, e.lo) || bounds(self, u, e.hi),
                }
            })
    }
}

/// Whether `s`, other than a type parameter, is defined from `born` on, or its owner is, in a
/// method or value.
fn is_local_since(u: &Universe, s: SymbolId, born: SymbolId) -> bool {
    if u.symbol(s).kind == Kind::TypeParam {
        return false;
    }
    let mut s = s;
    while let Some(o) = u.symbol(s).owner {
        if s.0 >= born.0 && matches!(u.symbol(o).kind, Kind::Def | Kind::Val) {
            return true;
        }
        s = o;
    }
    false
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
