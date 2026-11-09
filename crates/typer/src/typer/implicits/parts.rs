// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Implicits.scala
//! The parts of a type whose companions make up its implicit scope (dotty's `collectParts`).

use super::super::Typer;
use tasty::{Kind, SymbolId, Type, TypeId, TypeVars, Universe};

/// The classes and the abstract and opaque types of a type's parts, each with the prefix it is
/// referenced through if it is referenced directly.
#[derive(Default)]
pub struct Parts {
    pub classes: Vec<(SymbolId, Option<TypeId>)>,
    pub anchors: Vec<(SymbolId, Option<TypeId>)>,
    /// The types visited.
    seen: Vec<TypeId>,
}

impl Typer<'_> {
    /// The symbol reference `t` refers to.
    pub fn referenced(&mut self, t: TypeId) -> Option<SymbolId> {
        match self.u.types().get(t).clone() {
            Type::Sym(_, s) => Some(s),
            Type::TypeRef(p, n) => self.u.resolve(p, &n, true),
            Type::TermRef(p, n) => self.u.resolve(p, &n, false),
            _ => None,
        }
    }
}

impl Parts {
    /// The parts of `tpe` and of the type arguments of the parents of their classes.
    pub fn of(t: &mut Typer<'_>, tpe: TypeId) -> Parts {
        let mut parts = Parts::default();
        parts.collect(t, tpe);
        parts.collect_parent_args(t);
        parts
    }

    /// Adds the classes of the parts of `tpe` and the abstract and opaque types among them: of
    /// the references in `tpe` dealiased, of the types of its term references, of the bounds of
    /// its type parameters, and of the prefixes of its class, abstract type and term
    /// references; never of packages or package objects.
    fn collect(&mut self, t: &mut Typer<'_>, tpe: TypeId) {
        if self.seen.contains(&tpe) {
            return;
        }
        self.seen.push(tpe);
        let tpe = t.u.dealias(tpe);
        let prefix = match *t.u.types().get(tpe) {
            Type::Sym(p, _) => p,
            Type::TypeRef(p, _) | Type::TermRef(p, _) => Some(p),
            Type::This(c) => return self.collect(t, c),
            Type::Match { bound, .. } => return self.collect(t, bound),
            Type::Var(v) => {
                let (lo, hi) = t.constraint.bounds(v);
                self.collect(t, lo);
                return self.collect(t, hi);
            }
            _ => {
                for part in children(t.u, tpe) {
                    self.collect(t, part);
                }
                return;
            }
        };
        let Some(sym) = t.referenced(tpe) else { return };
        if is_excluded(t.u, sym) {
            return;
        }
        let class = match t.u.symbol(sym).kind.is_term() {
            true => {
                let widened = t.u.widen(tpe);
                if widened != tpe {
                    self.collect(t, widened);
                }
                None
            }
            false => t.u.class_symbol(tpe).map(|c| (c, prefix.filter(|_| c == sym))),
        };
        if let Some(c) = class.filter(|&(c, _)| !is_excluded(t.u, c))
            && !self.classes.contains(&c)
        {
            self.classes.push(c);
        }
        let param = t.u.symbol(sym).kind == Kind::TypeParam;
        if param {
            let info = t.u.info(sym);
            self.collect(t, info);
        }
        let alias = t.u.alias_of(sym).is_some();
        if t.u.symbol(sym).kind == Kind::Type && !alias && !self.anchors.contains(&(sym, prefix)) {
            self.anchors.push((sym, prefix));
        }
        if let Some(p) = prefix.filter(|_| !param && !alias) {
            self.collect(t, p);
        }
    }

    /// Adds the parts of the type arguments of the parents of the classes and of their base
    /// classes, as seen from the classes' prefixes (dotty's `iscopeRefs` of a class's parents);
    /// none for a class of an instance without its prefix.
    fn collect_parent_args(&mut self, t: &mut Typer<'_>) {
        let mut done = Vec::new();
        let mut i = 0;
        while let Some(&class) = self.classes.get(i) {
            let mut pending = vec![class];
            while let Some((c, prefix)) = pending.pop() {
                let owner = t.u.symbol(c).owner.filter(|&o| t.u.is_class(o));
                let instance = owner.is_some_and(|o| !t.u.is_module_class(o));
                if done.contains(&(c, prefix)) || prefix.is_none() && instance {
                    continue;
                }
                done.push((c, prefix));
                for parent in t.u.parents(c) {
                    let parent = match (prefix, owner) {
                        (Some(p), Some(o)) => t.u.as_seen_from(parent, p, o),
                        _ => parent,
                    };
                    let tycon = match t.u.types().get(parent).clone() {
                        Type::Applied(tycon, args) => {
                            for arg in args {
                                self.collect(t, arg);
                            }
                            tycon
                        }
                        _ => parent,
                    };
                    let tycon = t.u.dealias(tycon);
                    let p = match *t.u.types().get(tycon) {
                        Type::Sym(p, _) => p,
                        Type::TypeRef(p, _) => Some(p),
                        _ => continue,
                    };
                    if let Some(s) = t.referenced(tycon).filter(|&s| t.u.is_class(s)) {
                        pending.push((s, p));
                    }
                }
            }
            i += 1;
        }
    }
}

/// The types `t` is built from.
fn children(u: &mut Universe, t: TypeId) -> Vec<TypeId> {
    let mut out = Vec::new();
    u.map_type(t, &mut |_, part| {
        (part != t).then(|| {
            out.push(part);
            part
        })
    });
    out
}

/// Whether `s` is a package, a package object or its class (dotty's `isExcluded`).
pub fn is_excluded(u: &mut Universe, s: SymbolId) -> bool {
    let class = match u.is_module(s) {
        true => u.module_class(s),
        false => Some(s),
    };
    u.symbol(s).kind == Kind::Package || class.is_some_and(|c| crate::defn::is_named_package_object(u, c))
}
