//! Base classes that the parents of a class give different type arguments (the base type part of
//! dotty's `checkRealizableBounds`).

use crate::typer::Typer;
use syntax::ast::Span;
use tasty::tags::{ABSTRACT, CONTRAVARIANT, COVARIANT};
use tasty::{Kind, SymbolId, Type, TypeId};

impl Typer<'_> {
    /// Checks that the parents of class `cls` give each of its base classes one type: an error at
    /// `name_span` for a concrete class whose parents pass two different classes as an invariant
    /// argument, a fallback for any other difference.
    pub fn check_base_types(&mut self, cls: SymbolId, name_span: Span) {
        let parents = self.u.parents(cls);
        let concrete = !self.u.is_trait(cls) && !self.u.symbol(cls).flags.has(ABSTRACT);
        for base in self.u.linearization(cls) {
            if base == cls || self.u.type_params(base).is_empty() {
                continue;
            }
            let found: Vec<TypeId> = parents.iter().filter_map(|&p| self.u.base_type(p, base)).collect();
            let Some((&first, rest)) = found.split_first() else { continue };
            for &other in rest {
                match self.same_base_type(base, first, other) {
                    Some(true) => {}
                    Some(false) if concrete => {
                        let (a, b) = (self.u.show_user(first), self.u.show_user(other));
                        let name = self.u.symbol(cls).name.clone();
                        let msg = format!(
                            "class {name} cannot be instantiated since it has conflicting base types {a} and {b}"
                        );
                        self.error_at(name_span, msg);
                        return;
                    }
                    _ => {
                        self.unsupported_at(
                            name_span,
                            "parents giving a base class different type arguments",
                        );
                        return;
                    }
                }
            }
        }
    }

    /// Checks that class `cls` sees each parameterized trait its superclass extends as the
    /// superclass does: an error at `name_span` if not, a fallback if unknown (dotty's
    /// `checkParameterizedTraitsOK`).
    pub fn check_parameterized_traits(&mut self, cls: SymbolId, name_span: Span) {
        let Some(&sup) = self.u.parents(cls).first().filter(|_| !self.u.is_trait(cls)) else { return };
        let this = self.u.this_type(cls);
        for base in self.u.linearization(cls) {
            if self.u.type_params(base).is_empty()
                || !self.trait_takes_params(base)
                || !self.super_derives_from(cls, base)
            {
                continue;
            }
            let (Some(a), Some(b)) = (self.u.base_type(this, base), self.u.base_type(sup, base)) else {
                continue;
            };
            match self.same_base_type(base, a, b) {
                Some(true) => {}
                Some(false) => {
                    let (name, a, b) =
                        (self.u.symbol(base).name.clone(), self.u.show_user(a), self.u.show_user(b));
                    let msg = format!(
                        "illegal inheritance: parameterized base trait {name} has type {a}, but super has {b}"
                    );
                    self.error_at(name_span, msg);
                }
                None => self.unsupported_at(name_span, "parameterized base trait seen differently"),
            }
        }
    }

    /// Whether base types `a` and `b` of class `base` are the same, as their dealiased arguments
    /// are, `Some(false)` if an invariant argument is two different classes, `None` if unknown.
    fn same_base_type(&mut self, base: SymbolId, a: TypeId, b: TypeId) -> Option<bool> {
        if a == b {
            return Some(true);
        }
        let (Type::Applied(_, xs), Type::Applied(_, ys)) =
            (self.u.types().get(a).clone(), self.u.types().get(b).clone())
        else {
            return None;
        };
        let params = self.u.type_params(base);
        let mut same = true;
        for ((x, y), p) in xs.into_iter().zip(ys).zip(params) {
            let (x, y) = (self.u.dealias(x), self.u.dealias(y));
            if x == y {
                continue;
            }
            let flags = self.u.symbol(p).flags;
            let (cx, cy) = (self.class_ref(x)?, self.class_ref(y)?);
            if flags.has(COVARIANT) || flags.has(CONTRAVARIANT) || cx == cy {
                return None;
            }
            same = false;
        }
        Some(same)
    }

    /// The class that type `t` refers to without arguments.
    fn class_ref(&mut self, t: TypeId) -> Option<SymbolId> {
        let s = match *self.u.types().get(t) {
            Type::Sym(_, s) => s,
            Type::TypeRef(..) => self.u.resolve_ref(t)?,
            _ => return None,
        };
        (self.u.symbol(s).kind == Kind::Class).then_some(s)
    }
}
