// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/TypeOps.scala
//! Types of inlined trees widened to avoid their bindings (dotty's `TypeOps.avoid`).

use super::Inlining;
use tasty::tags::{CONTRAVARIANT, COVARIANT, INLINEPROXY};
use tasty::{SymbolId, Type, TypeId};

impl Inlining<'_> {
    /// `tpe` widened until it refers to none of `locals`; `None` if it cannot be.
    pub fn avoid(&mut self, mut tpe: TypeId, locals: &[SymbolId]) -> Option<TypeId> {
        while locals.iter().any(|&s| self.mentions(tpe, s)) {
            if let Some((Some(pre), s)) = self.u.sym_ref(tpe)
                && let Some(alias) = self.proxy_alias(pre, s, locals)
            {
                tpe = alias;
                continue;
            }
            let widened = self.u.widen_once(tpe).unwrap_or(tpe);
            let widened = self.u.by_name(widened).unwrap_or(widened);
            let widened = self.u.dealias(widened);
            let widened = crate::typer::simplify::normalize_all(self.u, widened);
            let widened = if widened == tpe { self.avoid_parts(tpe, locals) } else { widened };
            if widened == tpe {
                return None;
            }
            tpe = widened;
        }
        Some(tpe)
    }

    /// The alias of type member `s` that prefix `pre`, a chain of `locals` ending in a refined
    /// type, exposes (an opaque proxy's refinement).
    fn proxy_alias(&mut self, mut pre: TypeId, s: SymbolId, locals: &[SymbolId]) -> Option<TypeId> {
        while let Some((None, p)) = self.u.sym_ref(pre)
            && locals.contains(&p)
        {
            pre = self.u.info(p);
        }
        let name = self.u.interned_name(s);
        while let Type::Refined(parent, n, info) = *self.u.ty(pre) {
            if n == name {
                return self.u.type_alias(info);
            }
            pre = parent;
        }
        None
    }

    /// `tpe` with its parts that refer to `locals` replaced: a type selected from a path through
    /// one by its alias, a local value or a value selected from one by its type; `tpe` itself if
    /// a local value is misplaced.
    fn avoid_parts(&mut self, tpe: TypeId, locals: &[SymbolId]) -> TypeId {
        if self.misplaced_local(tpe, locals, 1, 16) {
            return tpe;
        }
        self.u.map_type(tpe, &mut |u, part| {
            let (p, x) = u.sym_ref(part)?;
            if locals.contains(&x) && !u.is_type_symbol(x) {
                return Some(u.info(x)).filter(|&i| i != part);
            }
            let p = p?;
            if !mentions_any(u, p, locals) {
                return None;
            }
            if !u.is_type_symbol(x) {
                return widened_value(u, part);
            }
            u.referenced_alias(part).filter(|&d| d != part)
        })
    }

    /// Whether a local value of `locals` occurs in `t`, at a position of variance `variance`
    /// (1 covariant, 0 invariant, -1 contravariant), where its type cannot stand for it: other
    /// than covariantly, or contravariantly for an inline proxy; a type or a covariant value
    /// selected from a path through one counts by its alias or its type.
    fn misplaced_local(&mut self, t: TypeId, locals: &[SymbolId], variance: i8, depth: u32) -> bool {
        if depth == 0 {
            return mentions_any(self.u, t, locals);
        }
        match *self.u.ty(t) {
            Type::Sym(p, x) if !self.u.is_type_symbol(x) => {
                if locals.contains(&x) {
                    let proxy = self.u.symbol(x).flags.has(INLINEPROXY);
                    return if proxy { variance < 0 } else { variance < 1 };
                }
                let Some(p) = p.filter(|&p| mentions_any(self.u, p, locals)) else { return false };
                match widened_value(self.u, t).filter(|_| variance > 0) {
                    Some(w) => self.misplaced_local(w, locals, variance, depth - 1),
                    None => self.misplaced_local(p, locals, 0, depth - 1),
                }
            }
            Type::Sym(..) | Type::TypeRef(..) => {
                let d = self.u.dealias(t);
                if d != t {
                    return self.misplaced_local(d, locals, variance, depth - 1);
                }
                let p = match *self.u.ty(t) {
                    Type::TypeRef(p, _) => Some(p),
                    Type::Sym(p, _) => p,
                    _ => None,
                };
                p.is_some_and(|p| self.misplaced_local(p, locals, 0, depth - 1))
            }
            Type::Applied(tycon, args) => {
                let class = self.u.class_symbol(tycon).filter(|&c| self.u.is_class(c));
                let params = class.map(|c| self.u.type_params(c)).unwrap_or_default();
                let mut found = self.misplaced_local(tycon, locals, 0, depth - 1);
                for i in 0..self.u.types_of(args).len() {
                    let a = self.u.types_of(args)[i];
                    let flags = params.get(i).map(|&p| self.u.symbol(p).flags);
                    let param = match flags {
                        Some(f) if f.has(COVARIANT) => 1,
                        Some(f) if f.has(CONTRAVARIANT) => -1,
                        _ => 0,
                    };
                    found = found || self.misplaced_local(a, locals, variance * param, depth - 1);
                }
                found
            }
            Type::Or(a, b, _) | Type::And(a, b) => {
                self.misplaced_local(a, locals, variance, depth - 1)
                    || self.misplaced_local(b, locals, variance, depth - 1)
            }
            _ => mentions_any(self.u, t, locals),
        }
    }

    /// Whether type `t` refers to symbol `s`.
    pub fn mentions(&mut self, t: TypeId, s: SymbolId) -> bool {
        mentions_any(self.u, t, &[s])
    }
}

/// The type of value reference `t`, a by-name result as its result; `None` if it has none.
fn widened_value(u: &mut tasty::Universe, t: TypeId) -> Option<TypeId> {
    let w = u.widen_once(t).filter(|&w| w != t)?;
    Some(u.by_name(w).unwrap_or(w))
}

/// Whether type `t` refers to one of `syms`.
fn mentions_any(u: &mut tasty::Universe, t: TypeId, syms: &[SymbolId]) -> bool {
    let mut found = false;
    u.map_type(t, &mut |u, part| {
        found |= u.sym_of(part).is_some_and(|x| syms.contains(&x));
        None
    });
    found
}
