// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Implicits.scala
//! The parts of a type whose companions make up its implicit scope (dotty's `collectParts`).

use super::super::Typer;
use tasty::{Kind, SymbolId, Type, TypeId, TypeVars};

/// The classes and the abstract and opaque types of a type's parts, each with the prefix it is
/// referenced through if it is referenced directly.
#[derive(Default)]
pub struct Parts {
    pub classes: Vec<(SymbolId, Option<TypeId>)>,
    pub anchors: Vec<(SymbolId, Option<TypeId>)>,
}

impl Typer<'_> {
    /// Adds to `parts` the classes of the parts of `t` and the abstract and opaque types among
    /// them: of the references in `t` dealiased, of the types of its term references, of the
    /// bounds of its type parameters, and of the prefixes of its class, abstract type and term
    /// references; never of packages or package objects.
    pub fn collect_parts(&mut self, t: TypeId, seen: &mut Vec<TypeId>, parts: &mut Parts) {
        if seen.contains(&t) {
            return;
        }
        seen.push(t);
        let t = self.u.dealias(t);
        let prefix = match *self.u.types().get(t) {
            Type::Sym(p, _) => p,
            Type::TypeRef(p, _) | Type::TermRef(p, _) => Some(p),
            Type::This(c) => return self.collect_parts(c, seen, parts),
            Type::Match { bound, .. } => return self.collect_parts(bound, seen, parts),
            Type::Var(v) => {
                let (lo, hi) = self.constraint.bounds(v);
                self.collect_parts(lo, seen, parts);
                return self.collect_parts(hi, seen, parts);
            }
            _ => {
                for part in self.children(t) {
                    self.collect_parts(part, seen, parts);
                }
                return;
            }
        };
        let Some(sym) = self.referenced(t) else { return };
        if self.is_excluded(sym) {
            return;
        }
        let class = match self.u.symbol(sym).kind.is_term() {
            true => {
                let widened = self.u.widen(t);
                if widened != t {
                    self.collect_parts(widened, seen, parts);
                }
                None
            }
            false => self.u.class_symbol(t).map(|c| (c, prefix.filter(|_| c == sym))),
        };
        if let Some(c) = class.filter(|&(c, _)| !self.is_excluded(c))
            && !parts.classes.contains(&c)
        {
            parts.classes.push(c);
        }
        let param = self.u.symbol(sym).kind == Kind::TypeParam;
        if param {
            let info = self.u.info(sym);
            self.collect_parts(info, seen, parts);
        }
        let alias = self.u.alias_of(sym).is_some();
        if self.u.symbol(sym).kind == Kind::Type && !alias && !parts.anchors.contains(&(sym, prefix)) {
            parts.anchors.push((sym, prefix));
        }
        if let Some(p) = prefix.filter(|_| !param && !alias) {
            self.collect_parts(p, seen, parts);
        }
    }

    /// Adds to `parts` the parts of the type arguments of the parents of its classes and of
    /// their base classes, as seen from the classes' prefixes (dotty's `iscopeRefs` of a class's
    /// parents); none for a class of an instance without its prefix.
    pub fn collect_parent_args(&mut self, seen: &mut Vec<TypeId>, parts: &mut Parts) {
        let mut done = Vec::new();
        let mut i = 0;
        while let Some(&class) = parts.classes.get(i) {
            let mut pending = vec![class];
            while let Some((c, prefix)) = pending.pop() {
                let owner = self.u.symbol(c).owner.filter(|&o| self.u.is_class(o));
                let instance = owner.is_some_and(|o| !self.u.is_module_class(o));
                if done.contains(&(c, prefix)) || prefix.is_none() && instance {
                    continue;
                }
                done.push((c, prefix));
                for parent in self.u.parents(c) {
                    let parent = match (prefix, owner) {
                        (Some(p), Some(o)) => self.u.as_seen_from(parent, p, o),
                        _ => parent,
                    };
                    let tycon = match self.u.types().get(parent).clone() {
                        Type::Applied(tycon, args) => {
                            for arg in args {
                                self.collect_parts(arg, seen, parts);
                            }
                            tycon
                        }
                        _ => parent,
                    };
                    let tycon = self.u.dealias(tycon);
                    let p = match *self.u.types().get(tycon) {
                        Type::Sym(p, _) => p,
                        Type::TypeRef(p, _) => Some(p),
                        _ => continue,
                    };
                    if let Some(s) = self.referenced(tycon).filter(|&s| self.u.is_class(s)) {
                        pending.push((s, p));
                    }
                }
            }
            i += 1;
        }
    }

    /// The symbol reference `t` refers to.
    pub fn referenced(&mut self, t: TypeId) -> Option<SymbolId> {
        match self.u.types().get(t).clone() {
            Type::Sym(_, s) => Some(s),
            Type::TypeRef(p, n) => self.u.resolve(p, &n, true),
            Type::TermRef(p, n) => self.u.resolve(p, &n, false),
            _ => None,
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
    pub fn is_excluded(&mut self, s: SymbolId) -> bool {
        let class = match self.u.is_module(s) {
            true => self.u.module_class(s),
            false => Some(s),
        };
        self.u.symbol(s).kind == Kind::Package || class.is_some_and(|c| self.is_named_package_object(c))
    }
}
