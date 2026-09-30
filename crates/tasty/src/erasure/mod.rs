//! Erasure of member types to what a method signature records (dotty's `TypeErasure`, as far
//! as the symbol model reaches), and Scala Native's mangled names from it.

mod applied;
mod mangle;

use crate::symbols::encode;
use crate::types::{Type, TypeId};
use crate::{Kind, SymbolId, Universe};

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

/// The erasure of a class by its qualified name, for classes the compiler defines itself or
/// erases specially.
fn special(full: &str) -> Option<Erased> {
    Some(match full {
        "scala.Boolean" => Erased::Prim('z'),
        "scala.Char" => Erased::Prim('c'),
        "scala.Byte" => Erased::Prim('b'),
        "scala.Short" => Erased::Prim('s'),
        "scala.Int" => Erased::Prim('i'),
        "scala.Long" => Erased::Prim('j'),
        "scala.Float" => Erased::Prim('f'),
        "scala.Double" => Erased::Prim('d'),
        "scala.Unit" => Erased::Unit,
        "scala.Nothing" => Erased::Nothing,
        "scala.Null" => Erased::Null,
        "scala.Any" | "scala.AnyVal" | "scala.AnyRef" | "scala.Matchable" | "scala.Singleton"
        | "scala.AnyKind" | "java.lang.Object" => Erased::Class(OBJECT.into()),
        "scala.<repeated>" => Erased::Class("scala.collection.immutable.Seq".into()),
        "scala.Tuple" | "scala.NonEmptyTuple" | "scala.*:" => Erased::Class("scala.Product".into()),
        _ => {
            let pure = full.strip_prefix("scala.Impure")?;
            return pure.contains("Function").then(|| Erased::Class(format!("scala.{pure}")));
        }
    })
}

/// Upper bounds whose type parameters make `Array[T]` a generic array, erased to `Object`.
const UNBOUNDED: [&str; 4] = ["scala.Any", "scala.AnyVal", "scala.Matchable", "scala.AnyKind"];

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
        match self.types.get(t).clone() {
            Type::Sym(_, s) => self.erase_symbol(s),
            Type::TypeRef(p, n) => match self.resolve(p, &n, true) {
                Some(s) => self.erase_symbol(s),
                None => match self.unresolved_class(p, &n) {
                    Some(full) => special(&full).unwrap_or(Erased::Class(full)),
                    None => Erased::Class(OBJECT.into()),
                },
            },
            Type::TermRef(p, n) => match self.resolve(p, &n, false) {
                Some(s) => {
                    let info = self.info(s);
                    self.erase(info)
                }
                None => Erased::Class(OBJECT.into()),
            },
            Type::Applied(tycon, args) => self.erase_applied(tycon, &args),
            Type::This(c) | Type::Alias(c) | Type::Bounds(_, c) | Type::Annotated(c) | Type::Rec(_, c) => {
                self.erase(c)
            }
            Type::Refined(p, _, _) => self.erase(p),
            Type::Lambda { result, .. } => self.erase(result),
            Type::ByName(_) => Erased::Class("scala.Function0".into()),
            Type::And(a, b) => self.erase_and(a, b),
            Type::Or(a, b) => self.erase_or(a, b),
            Type::Constant(tag, _) => constant_erasure(tag),
            _ => Erased::Class(OBJECT.into()),
        }
    }

    fn erase_symbol(&mut self, s: SymbolId) -> Erased {
        match self.symbol(s).kind {
            Kind::Class => {
                let full = self.full_name(s);
                special(&full).unwrap_or_else(|| Erased::Class(self.binary_name(s)))
            }
            Kind::Type | Kind::TypeParam => {
                let info = self.info(s);
                self.erase(info)
            }
            Kind::Val | Kind::Param => {
                let info = self.info(s);
                self.erase(info)
            }
            _ => Erased::Class(OBJECT.into()),
        }
    }

    /// The qualified name `prefix.name` stands for when no symbol defines it: a class of a
    /// package the universe lacks, or one the compiler defines.
    fn unresolved_class(&self, prefix: TypeId, name: &str) -> Option<String> {
        Some(format!("{}.{name}", self.path_name(prefix)?))
    }

    /// The qualified name of a package path: a package, one the universe lacks, or a term
    /// selected from one (`java` then `lang`).
    fn path_name(&self, t: TypeId) -> Option<String> {
        match self.types.get(t) {
            Type::Package(pkg) => Some(self.full_name(*pkg)),
            Type::PackageName(pkg) => Some(pkg.clone()),
            Type::TermRef(p, n) => Some(format!("{}.{n}", self.path_name(*p)?)),
            _ => None,
        }
    }

    /// The qualified name of the class `t` refers to, if it names one.
    fn class_name(&mut self, t: TypeId) -> Option<String> {
        match self.types.get(t).clone() {
            Type::Sym(_, s) if self.symbol(s).kind == Kind::Class => Some(self.full_name(s)),
            Type::TypeRef(p, n) => match self.resolve(p, &n, true) {
                Some(s) if self.symbol(s).kind == Kind::Class => Some(self.full_name(s)),
                Some(_) => None,
                None => self.unresolved_class(p, &n),
            },
            _ => None,
        }
    }
}

fn constant_erasure(tag: u8) -> Erased {
    use crate::tags::{
        BYTEconst, CHARconst, DOUBLEconst, FALSEconst, FLOATconst, INTconst, LONGconst, NULLconst,
        SHORTconst, STRINGconst, TRUEconst, UNITconst,
    };
    match tag {
        TRUEconst | FALSEconst => Erased::Prim('z'),
        BYTEconst => Erased::Prim('b'),
        SHORTconst => Erased::Prim('s'),
        CHARconst => Erased::Prim('c'),
        INTconst => Erased::Prim('i'),
        LONGconst => Erased::Prim('j'),
        FLOATconst => Erased::Prim('f'),
        DOUBLEconst => Erased::Prim('d'),
        UNITconst => Erased::Unit,
        NULLconst => Erased::Null,
        STRINGconst => Erased::Class("java.lang.String".into()),
        _ => Erased::Class(OBJECT.into()),
    }
}
