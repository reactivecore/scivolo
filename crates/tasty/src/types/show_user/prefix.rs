//! Paths and prefixes: packages in full; the root and empty packages, `scala`, `java.lang`,
//! `Predef` and REPL wrapper objects omitted as prefixes; package objects skipped.

use super::{Printer, simple};
use crate::tags::OBJECT;
use crate::types::{Type, TypeId};
use crate::{Kind, SymbolId, Universe};
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

fn is_package_object(name: &str) -> bool {
    let name = name.strip_suffix('$').unwrap_or(name);
    name == "package" || name.ends_with("$package")
}

/// The text of `path` as a prefix, `.` included; empty if the prefix is omitted.
pub fn prefix_text(path: Path) -> String {
    let last = path.full.rsplit('.').next().unwrap_or("");
    match OMITTED.contains(&path.full.as_str()) || last.contains("rs$line$") || path.shown.is_empty() {
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
            Type::PackageName(n) => Path { full: n.clone(), shown: n },
            Type::TermRef(pp, n) => {
                let prefix = self.path(pp);
                self.select(prefix, &n)
            }
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
            let full = self.u.full_name(s);
            return Path { shown: full.clone(), full };
        }
        let prefix = match prefix {
            Some(p) => self.path(p),
            None => self.owner_path(s),
        };
        let sym = self.u.symbol(s);
        let name = match sym.kind == Kind::Class && sym.flags.has(OBJECT) {
            true => sym.name.strip_suffix('$').unwrap_or(&sym.name).to_owned(),
            false => sym.name.clone(),
        };
        self.select(prefix, &name)
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

    /// Member `name` of path `prefix`.
    fn select(&mut self, prefix: Path, name: &str) -> Path {
        if is_package_object(name) {
            return prefix;
        }
        if matches!(name, "_root_" | "<root>" | "<empty>") {
            return Path::default();
        }
        let full = join(&prefix.full, name);
        if self.u.package(&full).is_some() {
            return Path { shown: full.clone(), full };
        }
        let shown = format!("{}{}", prefix_text(prefix), simple(name));
        Path { full, shown }
    }
}
