// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/TypeErasure.scala
//! Erasure of member types to what a method signature records (dotty's `TypeErasure`, as far
//! as the symbol model reaches), and Scala Native's mangled names from it.

mod applied;
mod bounds;
mod bridges;
mod compactify;
mod constructors;
mod generic;
mod mangle;
mod poly_functions;
mod scala2;
mod special;
mod value;

use crate::symbols::{defined_name, encode, module_class_name, module_name};
use crate::tags::{OBJECT as MODULE, PARAMsetter, TRAIT};
use crate::types::{Type, TypeId};
use crate::{Kind, SymbolId, Universe};
use compactify::compactify;
pub use special::PRIMITIVE_VALUE_CLASSES;
use special::special;

/// An erased type as Scala Native's backend sees it.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Erased {
    /// A primitive, by its mangled letter: `z c b s i j f d`, and Scala Native's raw pointer `R`
    /// and raw size `w`.
    Prim(char),
    Unit,
    Nothing,
    Null,
    /// A class by its binary name, e.g. `scala.Option`, `scala.Predef$`.
    Class(String),
    Array(Box<Erased>),
    /// A value class's value (dotty's `ErasedValueType`): the class's binary name and the
    /// erasure of its underlying value. Only `erase_value` makes it.
    Value(String, Box<Erased>),
}

const OBJECT: &str = "java.lang.Object";

fn object() -> Erased {
    Erased::Class(OBJECT.into())
}

impl Universe {
    /// The binary name of class `c`: `pkg.Name`, `Outer$Inner`, `Object$Inner`; names encoded,
    /// a JDK class Scala Native renames under its defined name when outer; a nested name
    /// `compactify`d.
    pub fn binary_name(&self, c: SymbolId) -> String {
        self.class_name_with(c, compactify)
    }

    /// `binary_name` without `compactify`, the prefix of an expanded name (`C$$$outer`).
    pub fn expanded_binary_name(&self, c: SymbolId) -> String {
        self.class_name_with(c, str::to_owned)
    }

    fn class_name_with(&self, c: SymbolId, compactify: fn(&str) -> String) -> String {
        let s = self.symbol(c);
        let name = encode(&s.name);
        match s.owner {
            Some(o) if self.symbol(o).kind == Kind::Class => {
                let outer = self.class_name_with(o, compactify);
                let module = module_name(&outer);
                let outer = match defined_name(module.unwrap_or(&outer)) {
                    Some(d) if module.is_some() => module_class_name(d),
                    Some(d) => d.to_owned(),
                    None => outer.clone(),
                };
                let (pkg, flat) = outer.rsplit_once('.').map_or(("", outer.as_str()), |(p, f)| (p, f));
                let module = s.flags.has(MODULE).then(|| module_name(&name)).flatten();
                let simple = module.unwrap_or(&name);
                let sep = if flat.ends_with('$') { "" } else { "$" };
                let flat =
                    compactify(&format!("{flat}{sep}{simple}")) + if module.is_some() { "$" } else { "" };
                if pkg.is_empty() { flat } else { format!("{pkg}.{flat}") }
            }
            Some(o) if o != self.root() => format!("{}.{name}", self.full_name(o)),
            _ => name,
        }
    }

    /// The erasure of type `t`.
    pub fn erase(&mut self, t: TypeId) -> Erased {
        match self.types().get(t).clone() {
            Type::Sym(_, s) => self.erase_symbol(s),
            Type::TypeRef(p, n) => match self.resolve_ref(t) {
                Some(s) => self.erase_symbol(s),
                None => match self.refinement_named(p, &n) {
                    Some(info) => self.erase(info),
                    None => match self.unresolved_class(p, &n) {
                        Some(full) => special(&full).unwrap_or(Erased::Class(full)),
                        None => object(),
                    },
                },
            },
            Type::TermRef(p, n) => match self.resolve_ref(t) {
                Some(s) => self.erase_symbol(s),
                None => match self.term_refinement_named(p, &n) {
                    Some(info) => match *self.types().get(info) {
                        Type::ByName(r) => self.erase(r),
                        _ => self.erase(info),
                    },
                    None => object(),
                },
            },
            Type::Applied(tycon, args) => self.erase_applied(tycon, &args),
            Type::This(c) | Type::Alias(c) | Type::Bounds(_, c) | Type::Annotated(c, _) | Type::Rec(_, c) => {
                self.erase(c)
            }
            Type::Match { bound, .. } => match self.try_normalize(t) {
                Some(r) => self.erase(r),
                None => self.erase(bound),
            },
            Type::Refined(p, _, _) => match self.poly_function_apply(t) {
                Some(info) => self.erase_refined_function_apply(info),
                None => self.erase(p),
            },
            Type::Skolem(_, p) => self.erase(p),
            Type::Lambda { result, .. } => self.erase(result),
            Type::ByName(_) => Erased::Class("scala.Function0".into()),
            Type::And(..) if self.scala2_erasure() => self.erase_scala2_and(t),
            Type::And(a, b) => self.erase_and(a, b),
            Type::Or(a, b, _) => self.erase_or(a, b),
            Type::Constant(tag, _) => special::constant(tag),
            Type::Str(_) => Erased::Class("java.lang.String".into()),
            _ => object(),
        }
    }

    /// The erasure of a reference to `s`: a class, an abstract type's bound, an alias, an
    /// object's class, a value's type.
    fn erase_symbol(&mut self, s: SymbolId) -> Erased {
        match self.symbol(s).kind {
            Kind::Class => {
                let full = self.full_name(s);
                if let Some(e) = special(&full) {
                    return e;
                }
                match self.underlying(s) {
                    Some(value) => {
                        let info = self.info(value);
                        self.erase(info)
                    }
                    None => Erased::Class(self.binary_name(s)),
                }
            }
            Kind::Val if self.symbol(s).flags.has(MODULE) => match self.module_class(s) {
                Some(c) => Erased::Class(self.binary_name(c)),
                None => object(),
            },
            Kind::Type | Kind::TypeParam | Kind::Val | Kind::Param => {
                let info = self.opaque_alias(s).unwrap_or_else(|| self.info(s));
                self.erase(info)
            }
            Kind::Def => {
                let info = self.info(s);
                match *self.types().get(info) {
                    Type::ByName(result) => self.erase(result),
                    Type::Method { .. } | Type::Poly { .. } => object(),
                    _ => self.erase(info),
                }
            }
            _ => object(),
        }
    }

    /// The underlying value of value class `c` (a class, not a trait, extending `AnyVal`): its
    /// first class parameter, or a script class's first parameter accessor, unless that has type
    /// `c` itself.
    pub fn underlying(&mut self, c: SymbolId) -> Option<SymbolId> {
        if self.symbol(c).flags.has(TRAIT) {
            return None;
        }
        let any_val = self.builtins().any_val?;
        if !self.parents(c).into_iter().any(|p| self.class_symbol(p) == Some(any_val)) {
            return None;
        }
        let value = self.symbol(c).decls.iter().copied().find(|&d| {
            let s = self.symbol(d);
            s.kind == Kind::Param || s.kind == Kind::Val && s.flags.has(PARAMsetter)
        })?;
        let info = self.info(value);
        (self.class_symbol(info) != Some(c)).then_some(value)
    }

    /// The erasure of `t` where values are boxed: a value class as itself.
    pub fn erase_boxed(&mut self, t: TypeId) -> Erased {
        match self.value_class(t) {
            Some(c) => Erased::Class(self.binary_name(c)),
            None => self.erase(t),
        }
    }

    /// The qualified name `prefix.name` stands for when no symbol defines it: a class of a
    /// package the universe lacks, a class nested in one (`Spliterator$OfInt`), or one the
    /// compiler defines.
    pub fn unresolved_class(&self, prefix: TypeId, name: &str) -> Option<String> {
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
            Type::TypeRef(p, n) => match self.resolve_ref(t) {
                Some(s) if self.symbol(s).kind == Kind::Class => Some(self.full_name(s)),
                Some(_) => None,
                None => self.unresolved_class(p, &n),
            },
            _ => None,
        }
    }
}
