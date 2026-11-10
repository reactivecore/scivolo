// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Implicits.scala
//! The parts of a type whose companions make up its implicit scope (dotty's `collectParts`).

use super::super::type_ops::TypeOps;
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

impl TypeOps<'_> {
    /// The symbol reference `t` refers to.
    pub fn referenced(&mut self, t: TypeId) -> Option<SymbolId> {
        match self.u.ty(t).clone() {
            Type::Sym(_, s) => Some(s),
            Type::TypeRef(p, n) => self.u.resolve(p, n, true),
            Type::TermRef(p, n) => self.u.resolve(p, n, false),
            _ => None,
        }
    }
}

impl Parts {
    /// The parts of `tpe` and of the type arguments of the parents of their classes.
    pub fn of(ty: &mut TypeOps<'_>, tpe: TypeId) -> Parts {
        let mut parts = Parts::default();
        parts.collect(ty, tpe);
        parts.collect_parent_args(ty);
        parts
    }

    /// Adds the classes of the parts of `tpe` and the abstract and opaque types among them: of
    /// the references in `tpe` dealiased, of the types of its term references, of the bounds of
    /// its type parameters, and of the prefixes of its class, abstract type and term
    /// references; never of packages or package objects.
    fn collect(&mut self, ty: &mut TypeOps<'_>, tpe: TypeId) {
        if self.seen.contains(&tpe) {
            return;
        }
        self.seen.push(tpe);
        let tpe = ty.u.dealias(tpe);
        let prefix = match *ty.u.ty(tpe) {
            Type::Sym(p, _) => p,
            Type::TypeRef(p, _) | Type::TermRef(p, _) => Some(p),
            Type::This(c) => return self.collect(ty, c),
            Type::Match { bound, .. } => return self.collect(ty, bound),
            Type::Var(v) => {
                let (lo, hi) = ty.constraint.bounds(v);
                self.collect(ty, lo);
                return self.collect(ty, hi);
            }
            _ => {
                for part in children(ty.u, tpe) {
                    self.collect(ty, part);
                }
                return;
            }
        };
        let Some(sym) = ty.referenced(tpe) else { return };
        if is_excluded(ty.u, sym) {
            return;
        }
        let class = match ty.u.symbol(sym).is_term() {
            true => {
                let widened = ty.u.widen(tpe);
                if widened != tpe {
                    self.collect(ty, widened);
                }
                None
            }
            false => ty.u.class_symbol(tpe).map(|c| (c, prefix.filter(|_| c == sym))),
        };
        if let Some(c) = class.filter(|&(c, _)| !is_excluded(ty.u, c))
            && !self.classes.contains(&c)
        {
            self.classes.push(c);
        }
        let param = ty.u.symbol(sym).kind == Kind::TypeParam;
        if param {
            let info = ty.u.info(sym);
            self.collect(ty, info);
        }
        let alias = ty.u.alias_of(sym).is_some();
        if ty.u.symbol(sym).kind == Kind::Type && !alias && !self.anchors.contains(&(sym, prefix)) {
            self.anchors.push((sym, prefix));
        }
        if let Some(p) = prefix.filter(|_| !param && !alias) {
            self.collect(ty, p);
        }
    }

    /// Adds the parts of the type arguments of the parents of the classes and of their base
    /// classes, as seen from the classes' prefixes (dotty's `iscopeRefs` of a class's parents);
    /// none for a class of an instance without its prefix.
    fn collect_parent_args(&mut self, ty: &mut TypeOps<'_>) {
        let mut done = Vec::new();
        let mut i = 0;
        while let Some(&class) = self.classes.get(i) {
            let mut pending = vec![class];
            while let Some((c, prefix)) = pending.pop() {
                let owner = ty.u.symbol(c).owner.filter(|&o| ty.u.is_class(o));
                let instance = owner.is_some_and(|o| !ty.u.is_module_class(o));
                if done.contains(&(c, prefix)) || prefix.is_none() && instance {
                    continue;
                }
                done.push((c, prefix));
                for parent in ty.u.parents(c) {
                    let parent = match (prefix, owner) {
                        (Some(p), Some(o)) => ty.u.as_seen_from(parent, p, o),
                        _ => parent,
                    };
                    let tycon = match ty.u.applied(parent).map(|(tycon, args)| (tycon, args.to_vec())) {
                        Some((tycon, args)) => {
                            for arg in args {
                                self.collect(ty, arg);
                            }
                            tycon
                        }
                        None => parent,
                    };
                    let tycon = ty.u.dealias(tycon);
                    let p = match *ty.u.ty(tycon) {
                        Type::Sym(p, _) => p,
                        Type::TypeRef(p, _) => Some(p),
                        _ => continue,
                    };
                    if let Some(s) = ty.referenced(tycon).filter(|&s| ty.u.is_class(s)) {
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
    u.symbol(s).is_package() || class.is_some_and(|c| crate::defn::is_named_package_object(u, c))
}
