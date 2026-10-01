//! Definitions outside the subset, or that dotty's checks (`Checking`, `RefChecks`) reject and the
//! typer does not report yet: both fall back.

use crate::typer::Typer;
use syntax::ast::NodeId;
use syntax::ast::flags::{self as f, FlagSet};
use tasty::{Kind, SymbolId};

impl Typer<'_> {
    /// Checks definition `tree` of `name` with the dotty flags `flags` of its modifiers in
    /// `owner`, before its symbol is entered.
    pub fn check_definition(
        &mut self,
        tree: NodeId,
        name: &str,
        kind: Kind,
        owner: SymbolId,
        flags: FlagSet,
    ) {
        if let Some(what) = self.definition_problem(name, kind, owner, flags) {
            self.unsupported(tree, what);
        }
    }

    fn definition_problem(
        &mut self,
        name: &str,
        kind: Kind,
        owner: SymbolId,
        flags: FlagSet,
    ) -> Option<&'static str> {
        let is_type = !kind.is_term();
        let applicable = if is_type { flags.is_type_flags() } else { flags.is_term_flags() };
        let combinations = [
            (f::FINAL, f::OPEN),
            (f::SEALED, f::OPEN),
            (f::FINAL, f::SEALED),
            (f::PRIVATE, f::PROTECTED),
            (f::ABSTRACT, f::OVERRIDE),
            (f::PRIVATE, f::OVERRIDE),
            (f::LAZY, f::INLINE),
            (f::ERASED, f::INLINE),
            (f::TRACKED, f::MUTABLE),
            (f::LAZY, f::PARAM_ACCESSOR),
        ];
        let flags = if is_type { flags.to_type_flags() } else { flags.to_term_flags() };
        let problem = if !applicable && !flags.is_empty() {
            "modifier incompatible with the definition"
        } else if combinations.iter().any(|&(a, b)| flags.is_all_of(a.or(b))) {
            "illegal combination of modifiers"
        } else if flags.is(f::INLINE) || flags.is(f::MACRO) {
            "inline definition"
        } else if flags.is(f::OPAQUE) {
            "opaque type"
        } else if flags.is(f::TRAIT) && flags.is_one_of(f::FINAL.or(f::OPEN)) {
            "final or open trait"
        } else if kind != Kind::Class && flags.is(f::ABSTRACT) {
            "abstract non-class"
        } else if kind == Kind::Val && !flags.is(f::TERM_PARAM) && name.ends_with("_=") {
            "value name ending in _="
        } else if flags.is_all_of(f::GIVEN.or(f::DEFERRED)) {
            "deferred given"
        } else if (kind != Kind::Def || name.contains("$default$")) && self.defines(owner, name, is_type) {
            "double definition"
        } else if self.u.symbol(owner).kind == Kind::Package && self.has_package(owner, name) {
            "definition named like a package"
        } else if kind == Kind::Class
            && self.u.symbol(owner).kind == Kind::Class
            && self.inherits_class(owner, name)
        {
            "class shadowing an inherited class"
        } else {
            return None;
        };
        Some(problem)
    }

    /// Whether package `owner` has a subpackage named `name`.
    fn has_package(&mut self, owner: SymbolId, name: &str) -> bool {
        let decls = self.u.decl(owner, name).unwrap_or_default();
        decls.into_iter().any(|d| self.u.symbol(d).kind == Kind::Package)
    }

    /// Whether this unit defined a term (or type) named `name` in `owner` already.
    fn defines(&self, owner: SymbolId, name: &str, is_type: bool) -> bool {
        let decls = &self.u.symbol(owner).decls;
        decls.iter().any(|&d| {
            self.syms.flags.contains_key(&d)
                && self.u.symbol(d).name == name
                && crate::symbols::is_type_symbol(self.u, d) == is_type
        })
    }

    /// Whether a base class of class `c` other than `c` has a member class named `name`.
    fn inherits_class(&mut self, c: SymbolId, name: &str) -> bool {
        let bases = self.u.linearization(c);
        bases.iter().skip(1).any(|&b| {
            let decls = &self.u.symbol(b).decls;
            decls.iter().any(|&d| self.u.symbol(d).kind == Kind::Class && self.u.symbol(d).name == name)
        })
    }

    /// Checks the parent types of the class with template `rhs`: no value class, no `Dynamic`, no
    /// final class, no sealed class of the library.
    pub fn check_parents(&mut self, rhs: NodeId, parents: &[tasty::TypeId]) {
        let any_val = self.u.builtins().any_val;
        for &p in parents {
            let Some(c) = self.u.class_symbol(p) else { continue };
            let flags = self.u.symbol(c).flags;
            let what = if Some(c) == any_val {
                "value class"
            } else if self.u.full_name(c) == "scala.Dynamic" {
                "Dynamic"
            } else if flags.has(tasty::tags::FINAL) || flags.has(tasty::tags::OBJECT) {
                "extending a final class"
            } else if flags.has(tasty::tags::SEALED) && !self.syms.flags.contains_key(&c) {
                "extending a sealed class of the library"
            } else {
                continue;
            };
            self.unsupported(rhs, what);
        }
    }

    /// Checks that annotation class `annot` is one the typer handles: `@main`.
    pub fn check_annotation(&mut self, annot: NodeId, class: Option<SymbolId>) {
        if class != Some(self.defn.main_annot) {
            self.unsupported(annot, "annotation");
        }
    }
}
