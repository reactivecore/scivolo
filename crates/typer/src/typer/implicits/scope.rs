// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Implicits.scala
//! The implicit scope of a type (dotty's `implicitScope`, `collectCompanions`): the implicits of
//! the companions of the classes of its parts and of their base classes, and of the values on
//! their prefix paths.

use super::super::Typer;
use super::candidates::Candidate;
use super::parts::Parts;
use tasty::tags::EXTENSION;
use tasty::{SymbolId, Type, TypeId};

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
        Ok(self.implicit_scope_of(t)?.implicits)
    }

    /// `implicit_scope` with the containers; `Err` if a companion with implicits or extension
    /// methods cannot be referenced.
    pub fn implicit_scope_of(&mut self, t: TypeId) -> Result<ImplicitScope, &'static str> {
        let t = self.zonk(t);
        let mut parts = Parts::default();
        self.collect_parts(t, &mut Vec::new(), &mut parts);
        let mut out = ImplicitScope::default();
        for (c, prefix) in parts.classes {
            let class_ref = match prefix {
                Some(p) => self.u.intern_type(Type::Sym(Some(p), c)),
                None => crate::defn::type_ref(self.u, c),
            };
            for base in self.u.linearization(c) {
                let prefix = match base == c {
                    true => prefix,
                    false => self.base_prefix(class_ref, base),
                };
                let term_path = prefix.filter(
                    |&p| matches!(*self.u.types().get(p), Type::Sym(_, s) if self.u.symbol(s).kind.is_term()),
                );
                let (module, module_ref) = match self.companion_module(base) {
                    Some(module) => match term_path.filter(|_| base == c) {
                        Some(p) => (module, self.u.intern_type(Type::Sym(Some(p), module))),
                        None => (module, crate::defn::type_ref(self.u, module)),
                    },
                    None => {
                        let Some(module) = self.class_companion(base) else { continue };
                        let path = prefix.filter(|&p| match *self.u.types().get(p) {
                            Type::This(_) => true,
                            Type::Sym(_, s) => self.u.symbol(s).kind.is_term(),
                            _ => false,
                        });
                        if base != c
                            && let Some(p) = path
                        {
                            self.add_path(p, &mut out)?;
                        }
                        match path {
                            Some(p) => (module, self.u.intern_type(Type::Sym(Some(p), module))),
                            None if self.holds_implicits(module) => {
                                return Err("implicit scope with an inner companion out of reach");
                            }
                            None => continue,
                        }
                    }
                };
                let Some(module_class) = self.u.module_class(module) else { continue };
                self.add_members(module_class, module_ref, &mut out);
            }
            if let Some(p) = prefix {
                self.add_static_module(p, &mut out);
                self.add_path(p, &mut out)?;
            }
        }
        for (s, prefix) in parts.anchors {
            if let Some(p) = prefix {
                self.add_static_module(p, &mut out);
                self.add_path(p, &mut out)?;
            }
            if let Some(m) = self.anchor_companion(s).filter(|&m| self.holds_implicits(m)) {
                let path = prefix.filter(|&p| match *self.u.types().get(p) {
                    Type::Sym(_, x) => self.u.symbol(x).kind.is_term(),
                    _ => false,
                });
                let (Some(p), Some(class)) = (path, self.u.module_class(m)) else {
                    return Err("implicit scope of an abstract or opaque type with a companion");
                };
                let module_ref = self.u.intern_type(Type::Sym(Some(p), m));
                self.add_members(class, module_ref, &mut out);
            }
        }
        Ok(out)
    }

    /// Whether object `module` has implicit members or extension methods.
    fn holds_implicits(&mut self, module: SymbolId) -> bool {
        let Some(class) = self.u.module_class(module) else { return false };
        !self.implicit_members(class, false).is_empty()
            || self.u.linearization(class).into_iter().any(|b| {
                let decls = self.u.decls(b).unwrap_or_default();
                decls.iter().any(|&d| self.u.symbol(d).flags.has(EXTENSION))
            })
    }

    /// The prefix of base class `base` in the base type of class reference `class_ref`.
    fn base_prefix(&mut self, class_ref: TypeId, base: SymbolId) -> Option<TypeId> {
        let mut t = self.u.base_type(class_ref, base)?;
        loop {
            match *self.u.types().get(t) {
                Type::Applied(tycon, _) => t = tycon,
                Type::Sym(p, _) => return p,
                _ => return None,
            }
        }
    }

    /// The companion object of class `c`, the object itself of a module class.
    fn class_companion(&mut self, c: SymbolId) -> Option<SymbolId> {
        match self.u.is_module_class(c) {
            true => self.u.source_module(c),
            false => self.u.companion_module(c),
        }
    }

    /// The object beside abstract or opaque type `s` that has its name.
    fn anchor_companion(&mut self, s: SymbolId) -> Option<SymbolId> {
        let owner = self.u.symbol(s).owner?;
        let name = self.u.symbol(s).name.clone();
        self.u.decl(owner, &name).ok()?.into_iter().find(|&m| self.u.is_module(m))
    }

    /// The companion object of class `c`, the object itself of a module class, if `c` is not
    /// nested in a class other than an object's.
    fn companion_module(&mut self, c: SymbolId) -> Option<SymbolId> {
        let owner = self.u.symbol(c).owner?;
        if self.is_instance_class(owner) {
            return None;
        }
        self.class_companion(c)
    }

    /// Whether `c` is a class other than an object's.
    fn is_instance_class(&self, c: SymbolId) -> bool {
        self.u.is_class(c) && !self.u.is_module_class(c)
    }
}
