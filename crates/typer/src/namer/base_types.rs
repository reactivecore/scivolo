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
                        let (a, b) = (self.show_type(first), self.show_type(other));
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

    /// Whether base types `a` and `b` of class `base` are the same, `Some(false)` if an invariant
    /// argument is two different classes, `None` if unknown.
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
            let (sx, sy) = (self.arg_symbol(x)?, self.arg_symbol(y)?);
            if sx == sy {
                continue;
            }
            let flags = self.u.symbol(p).flags;
            let classes = [sx, sy].iter().all(|&s| self.u.symbol(s).kind == Kind::Class);
            if flags.has(COVARIANT) || flags.has(CONTRAVARIANT) || !classes {
                return None;
            }
            same = false;
        }
        Some(same)
    }

    /// The symbol that dealiased type argument `t` refers to without arguments.
    fn arg_symbol(&mut self, t: TypeId) -> Option<SymbolId> {
        let t = self.u.dealias(t);
        match *self.u.types().get(t) {
            Type::Sym(_, s) => Some(s),
            Type::TypeRef(..) => self.u.resolve_ref(t),
            _ => None,
        }
    }
}
