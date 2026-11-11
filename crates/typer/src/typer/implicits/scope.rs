// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Implicits.scala
//! The implicit scope of a type (dotty's `implicitScope`, `collectCompanions`): the implicits of
//! the companions of the classes of its parts and of their base classes, and of the values on
//! their prefix paths.

use super::super::Typer;
use super::candidates::{Candidate, implicit_members};
use super::parts::Parts;
use tasty::{SymbolId, Type, TypeId, Universe};

/// The implicits of an implicit scope, and the objects and values they are members of (dotty's
/// `companionRefs`) as references.
#[derive(Default)]
pub struct ImplicitScope {
    pub implicits: Vec<Candidate>,
    pub containers: Vec<Candidate>,
}

impl Typer<'_> {
    /// The implicits of the companions of the classes `t` refers to, those of the objects it
    /// refers to, those of their base classes, and those of the stable values their prefixes go
    /// through.
    pub fn implicit_scope(&mut self, t: TypeId) -> Result<Vec<Candidate>, &'static str> {
        Ok(ImplicitScope::of(self, t)?.implicits)
    }
}

impl ImplicitScope {
    /// `implicit_scope` of `tpe` with the containers; `Err` if a companion with implicits or
    /// extension methods cannot be referenced.
    pub fn of(t: &mut Typer<'_>, tpe: TypeId) -> Result<ImplicitScope, &'static str> {
        let mut ty = t.types();
        let tpe = ty.zonk(tpe);
        let parts = Parts::of(&mut ty, tpe);
        let mut out = ImplicitScope::default();
        for (c, prefix) in parts.classes {
            out.add_class(t, c, prefix)?;
        }
        for (s, prefix) in parts.anchors {
            out.add_anchor(t, s, prefix)?;
        }
        Ok(out)
    }

    /// Adds the implicits of the companions of class `c` and its base classes, and of `prefix`.
    fn add_class(
        &mut self,
        t: &mut Typer<'_>,
        c: SymbolId,
        prefix: Option<TypeId>,
    ) -> Result<(), &'static str> {
        let class_ref = match prefix {
            Some(p) => t.u.intern_type(Type::Sym(Some(p), c)),
            None => crate::defn::type_ref(t.u, c),
        };
        for base in t.u.linearization(c) {
            let prefix = match base == c {
                true => prefix,
                false => base_prefix(t.u, class_ref, base),
            };
            let term_path = prefix.filter(|&p| match *t.u.ty(p) {
                Type::Sym(_, s) => t.u.symbol(s).is_term(),
                Type::Package(_) => true,
                _ => false,
            });
            let (module, module_ref) = match companion_module(t.u, base) {
                Some(module) => match term_path.filter(|_| base == c) {
                    Some(p) => (module, t.u.intern_type(Type::Sym(Some(p), module))),
                    None => (module, crate::defn::type_ref(t.u, module)),
                },
                None => {
                    let Some(module) = class_companion(t.u, base) else { continue };
                    let path = prefix.filter(|&p| match *t.u.ty(p) {
                        Type::This(_) => true,
                        Type::Sym(_, s) => t.u.symbol(s).is_term(),
                        _ => false,
                    });
                    if base != c
                        && let Some(p) = path
                    {
                        self.add_path(t, p)?;
                    }
                    match path {
                        Some(p) => (module, t.u.intern_type(Type::Sym(Some(p), module))),
                        None if holds_implicits(t, module) => {
                            return Err("implicit scope with an inner companion out of reach");
                        }
                        None => continue,
                    }
                }
            };
            let Some(module_class) = t.u.module_class(module) else { continue };
            self.add_members(t, module_class, module_ref);
        }
        if let Some(p) = prefix {
            self.add_static_module(t, p);
            self.add_path(t, p)?;
        }
        Ok(())
    }

    /// Adds the implicits of abstract or opaque type `s`'s companion, and of `prefix`.
    fn add_anchor(
        &mut self,
        t: &mut Typer<'_>,
        s: SymbolId,
        prefix: Option<TypeId>,
    ) -> Result<(), &'static str> {
        if let Some(p) = prefix {
            self.add_static_module(t, p);
            self.add_path(t, p)?;
        }
        if let Some(m) = anchor_companion(t.u, s).filter(|&m| holds_implicits(t, m)) {
            let path = prefix.and_then(|p| match *t.u.ty(p) {
                Type::Sym(_, x) => t.u.symbol(x).is_term().then_some(p),
                Type::TermRef(pp, _) => {
                    let x = t.types().referenced(p).filter(|&x| t.u.symbol(x).is_term())?;
                    Some(t.u.intern_type(Type::Sym(Some(pp), x)))
                }
                _ => None,
            });
            let (Some(p), Some(class)) = (path, t.u.module_class(m)) else {
                return Err("implicit scope of an abstract or opaque type with a companion");
            };
            let module_ref = t.u.intern_type(Type::Sym(Some(p), m));
            self.add_members(t, class, module_ref);
        }
        Ok(())
    }
}

/// Whether object `module` has implicit members or extension methods.
fn holds_implicits(t: &mut Typer<'_>, module: SymbolId) -> bool {
    let Some(class) = t.u.module_class(module) else { return false };
    !implicit_members(t, class, true).is_empty()
        || t.u.linearization(class).into_iter().any(|b| {
            let decls = t.u.decls(b).unwrap_or_default();
            decls.iter().any(|&d| t.u.symbol(d).is_extension())
        })
}

/// The prefix of base class `base` in the base type of class reference `class_ref`.
fn base_prefix(u: &mut Universe, class_ref: TypeId, base: SymbolId) -> Option<TypeId> {
    let mut t = u.base_type(class_ref, base)?;
    loop {
        match *u.ty(t) {
            Type::Applied(tycon, _) => t = tycon,
            Type::Sym(p, _) => return p,
            _ => return None,
        }
    }
}

/// The companion object of class `c`, the object itself of a module class.
fn class_companion(u: &mut Universe, c: SymbolId) -> Option<SymbolId> {
    match u.is_module_class(c) {
        true => u.source_module(c),
        false => u.companion_module(c),
    }
}

/// The object beside abstract or opaque type `s` that has its name.
fn anchor_companion(u: &mut Universe, s: SymbolId) -> Option<SymbolId> {
    let owner = u.symbol(s).owner?;
    let name = u.symbol(s).name.clone();
    u.decl(owner, &name).ok()?.into_iter().find(|&m| u.is_module(m))
}

/// The companion object of class `c`, the object itself of a module class, if `c` is not
/// nested in a class other than an object's.
fn companion_module(u: &mut Universe, c: SymbolId) -> Option<SymbolId> {
    let owner = u.symbol(c).owner?;
    if u.is_class(owner) && !u.is_module_class(owner) {
        return None;
    }
    class_companion(u, c)
}
