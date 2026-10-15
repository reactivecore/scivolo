//! Type variables that bound each other made one (dotty's `ConstraintHandling.unify`), and bounds
//! without the local symbols defined after a variable (`LevelAvoidMap`).

use super::Constraint;
use tasty::tags::PRIVATE;
use tasty::{ANY, Kind, NOTHING, SymbolId, Type, TypeId, TypeVars, Universe, VarId};

impl Constraint {
    /// For `v <: w` where `w <: v` holds already: one of the two instantiated to the other, which
    /// takes the bounds of both; `w` is kept unless `v` was made before more symbols were defined.
    /// False and unchanged if `w <: v` does not hold, the joined bounds are unsatisfiable or the
    /// removed one's bounds cannot do without the local symbols defined after the kept one.
    pub fn unify(&mut self, u: &mut Universe, v: VarId, w: VarId) -> bool {
        let (ev, ew) = (self.vars[v.0 as usize], self.vars[w.0 as usize]);
        if v == w || ew.instance.is_some() {
            return false;
        }
        let Some(hi_w) = without_var(u, ew.hi, v) else { return false };
        let hi_v = without_var(u, ev.hi, w).unwrap_or(ev.hi);
        let ((kept, ek, hk), (removed, er, hr)) = if ev.born.0 < ew.born.0 {
            ((v, ev, hi_v), (w, ew, hi_w))
        } else {
            ((w, ew, hi_w), (v, ev, hi_v))
        };
        let Some(lo_r) = self.avoided(u, er.lo, ek.born, 1) else { return false };
        let Some(hr) = self.avoided(u, hr, ek.born, -1) else { return false };
        let lo = join(u, ek.lo, lo_r);
        let hi = meet(u, hk, hr);
        let mark = self.mark();
        let kept_ref = u.intern_type(Type::Var(kept));
        self.instantiate(removed, kept_ref);
        self.set(kept, lo, hi);
        self.vars[kept.0 as usize].hard |= er.hard;
        let ok = hi == ANY || self.satisfiable(u, lo, hi);
        if !ok {
            self.rollback(mark);
        }
        ok
    }

    /// `t` at variance `v` without the local symbols defined from `born` on: a local class
    /// without members of its own where covariant replaced by its parents, a local value where
    /// covariant by its type, a variable made after `born` whose bounds refer to one where
    /// covariant by a new variable above it made at `born` (dotty's `legalVar`); `None` if
    /// another such symbol remains.
    pub fn avoided(&mut self, u: &mut Universe, t: TypeId, born: SymbolId, v: i32) -> Option<TypeId> {
        if !self.refers_to_local(u, t, born, 4, false) {
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
            Type::Sym(_, s) if v > 0 && is_local_since(u, s, born) => match u.symbol(s).kind {
                Kind::Class if owned_by_term(u, s) && !declares_members(u, s) => {
                    let parents = u.parents(s).into_iter().fold(ANY, |acc, p| meet(u, acc, p));
                    self.avoided(u, parents, born, v)
                }
                Kind::Val | Kind::Param => {
                    let widened = u.widen(t);
                    if widened == t { None } else { self.avoided(u, widened, born, v) }
                }
                _ => None,
            },
            Type::Applied(tycon, args) => {
                let class = u.class_symbol(tycon)?;
                let variances = u.variances(class);
                if variances.len() != args.len() || self.refers_to_local(u, tycon, born, 4, false) {
                    return None;
                }
                let mut out = Vec::with_capacity(args.len());
                for (&a, &pv) in args.iter().zip(&variances) {
                    if pv == 0 && self.refers_to_local(u, a, born, 4, false) {
                        return None;
                    }
                    out.push(self.avoided(u, a, born, v * i32::from(pv))?);
                }
                Some(u.intern_type(Type::Applied(tycon, out)))
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

    /// Whether `t` refers to a local symbol defined from `born` on, a type one if `types`, or has
    /// an uninstantiated variable made after `born` whose bounds do, `depth` variables deep.
    pub fn refers_to_local(
        &self,
        u: &mut Universe,
        t: TypeId,
        born: SymbolId,
        depth: u32,
        types: bool,
    ) -> bool {
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
        syms.into_iter().any(|s| is_local_since(u, s, born) && (!types || u.is_type_symbol(s)))
            || vars.into_iter().any(|x| {
                let e = self.vars[x.0 as usize];
                let bounds = |c: &Self, u: &mut Universe, b| c.refers_to_local(u, b, born, depth - 1, types);
                match e.instance {
                    Some(i) => depth == 0 || bounds(self, u, i),
                    None if e.born.0 <= born.0 => false,
                    None => depth == 0 || bounds(self, u, e.lo) || bounds(self, u, e.hi),
                }
            })
    }
}

/// `t` without the variable `v` among the parts of its top-level intersection, if `v` is one.
fn without_var(u: &mut Universe, t: TypeId, v: VarId) -> Option<TypeId> {
    match *u.types().get(t) {
        Type::Var(x) if x == v => Some(ANY),
        Type::And(x, y) => match (without_var(u, x, v), without_var(u, y, v)) {
            (Some(x), _) => Some(meet(u, x, y)),
            (_, Some(y)) => Some(meet(u, x, y)),
            _ => None,
        },
        _ => None,
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

fn join(u: &mut Universe, a: TypeId, b: TypeId) -> TypeId {
    if a == NOTHING || u.is_subtype(a, b) {
        b
    } else if b == NOTHING || u.is_subtype(b, a) {
        a
    } else {
        u.lub(a, b)
    }
}

fn meet(u: &mut Universe, a: TypeId, b: TypeId) -> TypeId {
    if a == ANY || u.is_subtype(b, a) {
        b
    } else if b == ANY || u.is_subtype(a, b) {
        a
    } else {
        u.glb(a, b)
    }
}
