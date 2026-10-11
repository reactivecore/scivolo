//! The implicit scope of a type (dotty's `implicitScope`): the implicits of the companions of the
//! classes of its parts and of their base classes.

use super::super::Typer;
use super::candidates::Candidate;
use tasty::{Kind, SymbolId, Type, TypeId};

impl Typer<'_> {
    /// The implicits of the companions of the classes `t` refers to, those of the objects it
    /// refers to, and those of their base classes.
    pub fn implicit_scope(&mut self, t: TypeId) -> Vec<Candidate> {
        let t = self.zonk(t);
        let mut classes = Vec::new();
        self.collect_parts(t, &mut Vec::new(), &mut classes);
        let mut out = Vec::new();
        for c in classes {
            for base in self.u.linearization(c) {
                let Some(module) = self.companion_module(base) else { continue };
                let Some(module_class) = self.u.module_class(module) else { continue };
                let prefix = Some(crate::defn::type_ref(self.u, module));
                for sym in self.implicit_members(module_class, false) {
                    let candidate = Candidate { sym, prefix };
                    if !out.contains(&candidate) {
                        out.push(candidate);
                    }
                }
            }
        }
        out
    }

    /// Adds to `classes` the classes of the parts of `t` (dotty's `collectParts`): of the
    /// references in `t` dealiased, of the types of its term references, and of the prefixes of
    /// its class, abstract type and term references; never of packages or package objects.
    fn collect_parts(&mut self, t: TypeId, seen: &mut Vec<TypeId>, classes: &mut Vec<SymbolId>) {
        if seen.contains(&t) {
            return;
        }
        seen.push(t);
        let t = self.u.dealias(t);
        let (prefix, sym) = match self.u.types().get(t).clone() {
            Type::Sym(p, s) => (p, s),
            Type::TypeRef(p, n) | Type::TermRef(p, n) => {
                let ty = matches!(self.u.types().get(t), Type::TypeRef(..));
                let Some(s) = self.u.resolve(p, &n, ty) else { return };
                (Some(p), s)
            }
            Type::This(c) => return self.collect_parts(c, seen, classes),
            _ => {
                for part in self.children(t) {
                    self.collect_parts(part, seen, classes);
                }
                return;
            }
        };
        if self.is_excluded(sym) {
            return;
        }
        let class = match self.u.symbol(sym).kind.is_term() {
            true => {
                let widened = self.u.widen(t);
                (widened != t).then(|| self.u.class_symbol(widened)).flatten()
            }
            false => self.u.class_symbol(t),
        };
        if let Some(c) = class.filter(|&c| !self.is_excluded(c))
            && !classes.contains(&c)
        {
            classes.push(c);
        }
        let param = self.u.symbol(sym).kind == Kind::TypeParam;
        if let Some(p) = prefix.filter(|_| !param && self.u.alias_of(sym).is_none()) {
            self.collect_parts(p, seen, classes);
        }
    }

    /// The types `t` is built from.
    fn children(&mut self, t: TypeId) -> Vec<TypeId> {
        let mut out = Vec::new();
        self.u.map_type(t, &mut |_, part| {
            (part != t).then(|| {
                out.push(part);
                part
            })
        });
        out
    }

    /// Whether `s` is a package, a package object or its class (dotty's `isExcluded`).
    fn is_excluded(&mut self, s: SymbolId) -> bool {
        let class = match self.u.is_module(s) {
            true => self.u.module_class(s),
            false => Some(s),
        };
        self.u.symbol(s).kind == Kind::Package || class.is_some_and(|c| self.is_named_package_object(c))
    }

    /// The companion object of class `c`, the object itself of a module class, if `c` is not
    /// nested in a class other than an object's.
    fn companion_module(&mut self, c: SymbolId) -> Option<SymbolId> {
        let owner = self.u.symbol(c).owner?;
        if self.u.is_class(owner) && !self.u.is_module_class(owner) {
            return None;
        }
        if self.u.is_module_class(c) {
            return self.u.source_module(c);
        }
        self.u.companion_module(c)
    }
}
