// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/printing/PlainPrinter.scala
//   compiler/src/dotty/tools/dotc/printing/RefinedPrinter.scala
//! The prefix of a reference as dotty's `RefinedPrinter.toTextPrefixOf` shows it in a message,
//! and the place of a member as `showLocated` shows it.

use super::Typer;
use tasty::tags::{OBJECT, TRAIT};
use tasty::{Kind, SymbolId, Type, TypeId, nme};

impl Typer<'_> {
    /// Where dotty's `showLocated` places a member of `owner`: ` in object O`, or nothing for a
    /// top-level or local definition.
    pub fn location(&mut self, owner: SymbolId) -> Option<String> {
        let o = self.u.symbol(owner);
        let (name, kind, flags, parent) = (o.name.clone(), o.kind, o.flags, o.owner);
        Some(match kind {
            Kind::Def | Kind::Val => String::new(),
            _ if self.u.is_package_object(owner) || name.starts_with("rs$line$") => {
                let p = parent?;
                match p == self.u.root() || self.u.symbol(p).name == nme::EMPTY_PACKAGE {
                    true => String::new(),
                    false => return None,
                }
            }
            Kind::Package if name == nme::EMPTY_PACKAGE || owner == self.u.root() => String::new(),
            Kind::Class if name.contains(nme::ANON_CLASS) => return None,
            Kind::Class if flags.has(OBJECT) => format!(" in object {}", tasty::module_name(&name)?),
            Kind::Class if flags.has(TRAIT) => format!(" in trait {name}"),
            Kind::Class if !name.contains('$') => format!(" in class {name}"),
            _ => return None,
        })
    }

    /// `prefix` shown with its separator, `""` if the printer omits it.
    pub fn shown_prefix(&mut self, prefix: TypeId) -> String {
        let cls = match *self.u.ty(prefix) {
            Type::Package(p) => Some(p),
            Type::This(c) => match *self.u.ty(c) {
                Type::Package(p) => Some(p),
                _ => self.u.class_symbol(c),
            },
            _ => None,
        };
        match cls {
            Some(cls) => self.shown_this_prefix(cls),
            None => String::new(),
        }
    }

    /// The prefix `cls.this` shown with its separator.
    fn shown_this_prefix(&mut self, cls: SymbolId) -> String {
        if self.is_named_package_object(cls) {
            return self.u.symbol(cls).owner.map_or_else(String::new, |p| self.shown_this_prefix(p));
        }
        if self.is_omittable_prefix(cls) {
            return String::new();
        }
        format!("{}.", self.shown_this(cls))
    }

    /// `cls.this` as dotty's `toTextRef` shows it.
    fn shown_this(&mut self, cls: SymbolId) -> String {
        let sym = self.u.symbol(cls);
        if sym.kind == Kind::Package {
            return self.full_name_string(cls);
        }
        if !self.u.is_module_class(cls) {
            return format!("{}.this", sym.name);
        }
        let name = tasty::module_name(&sym.name).unwrap_or(&sym.name).to_owned();
        if self.u.is_static_owner(cls) {
            return self.u.source_module(cls).map_or(name.clone(), |m| self.full_name_string(m));
        }
        match sym.owner.filter(|&o| self.u.is_class(o)) {
            Some(o) => format!("{}.{name}", self.shown_this(o)),
            None => name,
        }
    }

    /// dotty's `fullNameString`: the names from the outermost package or object, the empty
    /// package and omittable owners left out.
    fn full_name_string(&mut self, s: SymbolId) -> String {
        let sym = self.u.symbol(s);
        let name = match self.u.is_module_class(s) {
            true => tasty::module_name(&sym.name).unwrap_or(&sym.name).to_owned(),
            false => sym.name.clone(),
        };
        let Some(owner) = sym.owner else { return name };
        let package = sym.kind == Kind::Package;
        if self.is_empty_package(owner) || !package && self.is_omittable_prefix(owner) {
            return name;
        }
        let owner = match self.u.is_module_class(owner) {
            true => self.u.source_module(owner).unwrap_or(owner),
            false => owner,
        };
        format!("{}.{name}", self.full_name_string(owner))
    }

    /// Whether the printer leaves out a prefix of `cls`: the root and empty packages, `scala`,
    /// `java.lang` and `Predef` (dotty's `isOmittablePrefix`).
    fn is_omittable_prefix(&mut self, cls: SymbolId) -> bool {
        let full = self.u.full_name(cls);
        self.is_empty_package(cls)
            || matches!(full.as_str(), "scala" | "java.lang" | "scala.Predef$" | "scala.Predef")
    }

    fn is_empty_package(&self, s: SymbolId) -> bool {
        let sym = self.u.symbol(s);
        sym.kind == Kind::Package
            && (sym.owner.is_none() || matches!(sym.name.as_str(), "" | nme::EMPTY_PACKAGE | "<root>"))
    }
}
