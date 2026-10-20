// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/printing/PlainPrinter.scala
//   compiler/src/dotty/tools/dotc/printing/RefinedPrinter.scala
//   compiler/src/dotty/tools/dotc/printing/ReplPrinter.scala
//! Paths and prefixes: the root and empty packages, `scala`, `java.lang`, `Predef` and REPL
//! wrapper objects omitted as prefixes, of packages too (`collection.Seq`); package objects skipped.

use super::{Printer, simple};
use crate::tags::OBJECT;
use crate::types::{Type, TypeId};
use crate::{Kind, SymbolId, Universe, is_package_object_name, module_class_name, module_name};
use rustc_hash::FxHashSet;

/// A path by its qualified name and by its text.
#[derive(Default)]
pub struct Path {
    pub full: String,
    pub shown: String,
}

const OMITTED: [&str; 4] = ["", "scala", "java.lang", "scala.Predef"];

pub fn join(prefix: &str, name: &str) -> String {
    match prefix.is_empty() {
        true => name.to_owned(),
        false => format!("{prefix}.{name}"),
    }
}

/// Whether `name` is a package object's class or value.
fn is_package_object(name: &str) -> bool {
    is_package_object_name(name) || is_package_object_name(&module_class_name(name))
}

/// Whether a prefix of qualified name `full` is omitted, as one the root imports bring.
pub fn is_omitted(full: &str) -> bool {
    OMITTED.contains(&full)
}

/// Qualified name `full` after its last REPL wrapper object: `O` for `rs$line$3.O`.
pub fn after_wrapper(full: &str) -> &str {
    match full.rfind("rs$line$") {
        Some(at) => full[at..].split_once('.').map_or("", |(_, rest)| rest),
        None => full,
    }
}

/// The text of `path` as a prefix, `.` included; empty if the prefix is omitted.
pub fn prefix_text(path: Path) -> String {
    let last = path.full.rsplit('.').next().unwrap_or("");
    match is_omitted(&path.full) || last.contains("rs$line$") || path.shown.is_empty() {
        true => String::new(),
        false => format!("{}.", path.shown),
    }
}

/// The classes that type aliases of `Predef` and the `scala` package object name, and `Object`.
pub fn bare_classes(u: &mut Universe) -> FxHashSet<SymbolId> {
    let mut bare = FxHashSet::from_iter([u.builtins().object]);
    let Some(scala) = u.package("scala") else { return bare };
    for owner in ["Predef$", "package$"] {
        let decls = u.decl(scala, owner).unwrap_or_default();
        let Some(owner) = decls.into_iter().find(|&c| u.symbol(c).kind == Kind::Class) else { continue };
        for d in u.decls(owner).unwrap_or_default() {
            if u.symbol(d).kind == Kind::Type
                && let Some(alias) = u.alias_of(d)
                && let Some(class) = u.class_symbol(alias)
            {
                bare.insert(class);
            }
        }
    }
    bare
}

impl Printer<'_> {
    /// The text of prefix `p`, `.` included.
    pub fn prefix(&mut self, p: TypeId) -> String {
        let path = self.path(p);
        prefix_text(path)
    }

    /// Path `p`, a package, a value, a module or `this`.
    pub fn path(&mut self, p: TypeId) -> Path {
        match self.u.types().get(p).clone() {
            Type::Package(s) => self.symbol_path(None, s),
            Type::PackageName(n) => package_path(&n),
            Type::TermRef(pp, n) => select(self.path(pp), &n),
            Type::Sym(pp, s) => self.symbol_path(pp, s),
            Type::This(c) => match self.u.class_symbol(c) {
                Some(cls) if self.u.symbol(cls).flags.has(OBJECT) => self.symbol_path(None, cls),
                Some(cls) => self.this_path(cls),
                None => self.path(c),
            },
            _ => {
                let shown = self.text(p, super::DOT);
                Path { full: shown.clone(), shown }
            }
        }
    }

    /// The path of symbol `s` through `prefix`, or else its owners.
    pub fn symbol_path(&mut self, prefix: Option<TypeId>, s: SymbolId) -> Path {
        if self.u.symbol(s).kind == Kind::Package {
            return package_path(&self.u.full_name(s));
        }
        let prefix = match prefix {
            Some(p) => self.path(p),
            None => self.owner_path(s),
        };
        let sym = self.u.symbol(s);
        let name = match self.u.is_module_class(s) {
            true => module_name(&sym.name).unwrap_or(&sym.name).to_owned(),
            false => sym.name.clone(),
        };
        select(prefix, &name)
    }

    /// The path of the owner of `s`, empty for a local symbol.
    pub fn owner_path(&mut self, s: SymbolId) -> Path {
        let Some(owner) = self.u.symbol(s).owner else { return Path::default() };
        let o = self.u.symbol(owner);
        match o.kind {
            Kind::Package => self.symbol_path(None, owner),
            Kind::Class if o.flags.has(OBJECT) => self.symbol_path(None, owner),
            Kind::Class => self.this_path(owner),
            _ => Path::default(),
        }
    }

    fn this_path(&self, class: SymbolId) -> Path {
        Path {
            full: format!("{}.this", self.u.full_name(class)),
            shown: format!("{}.this", simple(&self.u.symbol(class).name)),
        }
    }
}

/// Package `full` through its owners: `java.util`, `collection.immutable`.
fn package_path(full: &str) -> Path {
    match full.rsplit_once('.') {
        Some((owner, name)) => select(package_path(owner), name),
        None => select(Path::default(), full),
    }
}

/// Member `name` of path `prefix`.
fn select(prefix: Path, name: &str) -> Path {
    if is_package_object(name) {
        return prefix;
    }
    if matches!(name, "" | "_root_" | "<root>" | "<empty>") {
        return Path::default();
    }
    let full = join(&prefix.full, name);
    let shown = format!("{}{}", prefix_text(prefix), simple(name));
    Path { full, shown }
}
