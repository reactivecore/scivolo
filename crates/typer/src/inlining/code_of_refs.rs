// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/printing/PlainPrinter.scala
//   compiler/src/dotty/tools/dotc/printing/RefinedPrinter.scala
//! The prefixes and types of `codeOf`'s text.

use super::Inlining;
use tasty::{Kind, SymbolId, Type, TypeId};

impl Inlining<'_> {
    /// The prefix of a reference to `s` through `prefix`, with its dot, empty where dotty's
    /// `RefinedPrinter.toTextPrefixOf` leaves it out.
    pub fn prefix_code(&mut self, prefix: Option<TypeId>, s: SymbolId) -> Option<String> {
        let Some(p) = prefix else {
            let owner = self.u.symbol(s).owner?;
            if self.u.symbol(owner).kind != Kind::Package || self.is_omitted(owner) {
                return Some(String::new());
            }
            return Some(format!("{}.", self.shown_full_name(owner)));
        };
        let omitted = match *self.u.types().get(p) {
            Type::Package(s) | Type::Sym(_, s) => self.is_omitted(s),
            Type::This(c) => self.u.class_symbol(c).is_some_and(|c| self.is_omitted(c)),
            _ => false,
        };
        match omitted {
            true => Some(String::new()),
            false => Some(format!("{}.", self.ref_code(p)?)),
        }
    }

    /// Singleton type `p` as a path (dotty's `RefinedPrinter.toTextRef`).
    fn ref_code(&mut self, p: TypeId) -> Option<String> {
        match *self.u.types().get(p) {
            Type::Package(s) => Some(self.shown_full_name(s)),
            Type::This(c) => {
                let c = self.u.class_symbol(c)?;
                let name = self.u.symbol(c).name.clone();
                if name.starts_with("$anon") {
                    Some("this".into())
                } else if self.u.is_module_class(c) {
                    let shown = self.u.is_static_symbol(c) && !self.u.is_package_object(c);
                    shown.then(|| self.shown_full_name(c))
                } else {
                    Some(format!("{name}.this"))
                }
            }
            Type::Sym(_, s) if self.u.symbol(s).kind == Kind::Package => Some(self.shown_full_name(s)),
            Type::Sym(prefix, s) if self.u.symbol(s).kind.is_term() => {
                let name = self.u.symbol(s).name.clone();
                Some(format!("{}{name}", self.prefix_code(prefix, s)?))
            }
            _ => None,
        }
    }

    /// Type `t` as dotty's `PlainPrinter.toText` shows it, if it refers to a class or alias.
    pub fn type_code(&mut self, t: TypeId) -> Option<String> {
        let Type::Sym(prefix, s) = *self.u.types().get(t) else { return None };
        if !self.u.is_type_symbol(s) || self.u.is_module_class(s) {
            return None;
        }
        let name = self.u.symbol(s).name.clone();
        Some(format!("{}{name}", self.prefix_code(prefix, s)?))
    }

    /// Whether dotty leaves out a prefix of `s`: `scala`, `java.lang`, `Predef`, the root and the
    /// empty package (`isOmittablePrefix`).
    fn is_omitted(&self, s: SymbolId) -> bool {
        let full = self.u.full_name(s);
        matches!(
            full.as_str(),
            "scala" | "java.lang" | "scala.Predef" | "scala.Predef$" | "" | "<empty>" | "<root>"
        )
    }

    /// The names of `s` and its owners below the root and the empty package, an object's without
    /// its `$` (dotty's `fullNameString` of a source module).
    fn shown_full_name(&self, s: SymbolId) -> String {
        let mut names = Vec::new();
        let mut at = Some(s);
        while let Some(x) = at.filter(|&x| x != self.u.root()) {
            let sym = self.u.symbol(x);
            if sym.kind == Kind::Package && sym.name == "<empty>" {
                break;
            }
            names.push(sym.name.strip_suffix('$').unwrap_or(&sym.name));
            at = sym.owner;
        }
        names.reverse();
        names.join(".")
    }
}
