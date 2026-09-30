//! Erasure of member types to what a method signature records (dotty's `TypeErasure`, as far
//! as the symbol model reaches), and Scala Native's mangled names from it.

mod applied;
mod bounds;
mod mangle;
mod special;

use crate::symbols::encode;
use crate::tags::OBJECT as MODULE;
use crate::types::{Type, TypeId};
use crate::{Kind, SymbolId, Universe};
use special::special;

/// An erased type as Scala Native's backend sees it.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Erased {
    /// A primitive, by its mangled letter: `z c b s i j f d`.
    Prim(char),
    Unit,
    Nothing,
    Null,
    /// A class by its binary name, e.g. `scala.Option`, `scala.Predef$`.
    Class(String),
    Array(Box<Erased>),
}

const OBJECT: &str = "java.lang.Object";

fn object() -> Erased {
    Erased::Class(OBJECT.into())
}

impl Universe {
    /// The binary name of class `c`: `pkg.Name`, `Outer$Inner`, `Object$Inner`; names encoded.
    pub fn binary_name(&self, c: SymbolId) -> String {
        let s = self.symbol(c);
        let name = encode(&s.name);
        match s.owner {
            Some(o) if self.symbol(o).kind == Kind::Class => {
                let outer = self.binary_name(o);
                if outer.ends_with('$') { format!("{outer}{name}") } else { format!("{outer}${name}") }
            }
            Some(o) if o != self.root() => format!("{}.{name}", self.full_name(o)),
            _ => name,
        }
    }

    /// The erasure of type `t`.
    pub fn erase(&mut self, t: TypeId) -> Erased {
        match self.types().get(t).clone() {
            Type::Sym(_, s) => self.erase_symbol(s),
            Type::TypeRef(p, n) => match self.resolve(p, &n, true) {
                Some(s) => self.erase_symbol(s),
                None => match self.unresolved_class(p, &n) {
                    Some(full) => special(&full).unwrap_or(Erased::Class(full)),
                    None => object(),
                },
            },
            Type::TermRef(p, n) => match self.resolve(p, &n, false) {
                Some(s) => self.erase_symbol(s),
                None => object(),
            },
            Type::Applied(tycon, args) => self.erase_applied(tycon, &args),
            Type::This(c) | Type::Alias(c) | Type::Bounds(_, c) | Type::Annotated(c) | Type::Rec(_, c) => {
                self.erase(c)
            }
            Type::Refined(p, _, _) | Type::Match(p) => self.erase(p),
            Type::Lambda { result, .. } => self.erase(result),
            Type::ByName(_) => Erased::Class("scala.Function0".into()),
            Type::And(a, b) => self.erase_and(a, b),
            Type::Or(a, b) => self.erase_or(a, b),
            Type::Constant(tag, _) => special::constant(tag),
            _ => object(),
        }
    }

    /// The erasure of a reference to `s`: a class, an abstract type's bound, an alias, an
    /// object's class, a value's type.
    fn erase_symbol(&mut self, s: SymbolId) -> Erased {
        match self.symbol(s).kind {
            Kind::Class => {
                let full = self.full_name(s);
                special(&full).unwrap_or_else(|| Erased::Class(self.binary_name(s)))
            }
            Kind::Val if self.symbol(s).flags.has(MODULE) => match self.module_class(s) {
                Some(c) => Erased::Class(self.binary_name(c)),
                None => object(),
            },
            Kind::Type | Kind::TypeParam | Kind::Val | Kind::Param => {
                let info = self.info(s);
                self.erase(info)
            }
            _ => object(),
        }
    }

    /// The qualified name `prefix.name` stands for when no symbol defines it: a class of a
    /// package the universe lacks, a class nested in one (`Spliterator$OfInt`), or one the
    /// compiler defines.
    fn unresolved_class(&self, prefix: TypeId, name: &str) -> Option<String> {
        let (path, in_class) = self.path_name(prefix)?;
        Some(format!("{path}{}{name}", if in_class { '$' } else { '.' }))
    }

    /// The qualified name of a path the universe cannot resolve, and whether it ends in a class
    /// or object (a capitalized term) rather than a package.
    fn path_name(&self, t: TypeId) -> Option<(String, bool)> {
        match self.types().get(t) {
            Type::Package(pkg) => Some((self.full_name(*pkg), false)),
            Type::PackageName(pkg) => Some((pkg.clone(), false)),
            Type::TermRef(p, n) => {
                let (path, in_class) = self.path_name(*p)?;
                let sep = if in_class { '$' } else { '.' };
                Some((format!("{path}{sep}{n}"), n.starts_with(char::is_uppercase)))
            }
            _ => None,
        }
    }

    /// The qualified name of the class `t` refers to, if it names one.
    fn class_name(&mut self, t: TypeId) -> Option<String> {
        match self.types().get(t).clone() {
            Type::Sym(_, s) if self.symbol(s).kind == Kind::Class => Some(self.full_name(s)),
            Type::TypeRef(p, n) => match self.resolve(p, &n, true) {
                Some(s) if self.symbol(s).kind == Kind::Class => Some(self.full_name(s)),
                Some(_) => None,
                None => self.unresolved_class(p, &n),
            },
            _ => None,
        }
    }

    /// The class `t` refers to, if the universe has it.
    fn class_symbol(&mut self, t: TypeId) -> Option<SymbolId> {
        match self.types().get(t).clone() {
            Type::Sym(_, s) if self.symbol(s).kind == Kind::Class => Some(s),
            Type::TypeRef(p, n) => self.resolve(p, &n, true).filter(|&s| self.symbol(s).kind == Kind::Class),
            Type::Applied(tycon, _) | Type::Annotated(tycon) => self.class_symbol(tycon),
            _ => None,
        }
    }
}
