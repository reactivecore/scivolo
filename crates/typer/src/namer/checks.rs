// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Checking.scala
//! Definitions outside the subset, or that dotty's checks (`Checking`, `RefChecks`) reject and the
//! typer does not report yet: both fall back.

use crate::ctx::Ctx;
use crate::typer::Typer;
use syntax::ast::NodeId;
use syntax::ast::flags::{self as f, FlagSet};
use tasty::{Kind, SymbolId};

impl Typer<'_> {
    /// Checks definition `tree` of `name` with the dotty flags `flags` of its modifiers in
    /// `ctx`, before its symbol is entered.
    pub fn check_definition(&mut self, tree: NodeId, name: &str, kind: Kind, ctx: Ctx, flags: FlagSet) {
        if let Some(what) = self.definition_problem(tree, name, kind, ctx, flags) {
            self.unsupported(tree, what);
        }
    }

    fn definition_problem(
        &mut self,
        tree: NodeId,
        name: &str,
        kind: Kind,
        ctx: Ctx,
        flags: FlagSet,
    ) -> Option<&'static str> {
        let owner = ctx.owner;
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
        } else if let Some(problem) = self.inline_problem(kind, owner, flags) {
            problem
        } else if flags.is(f::OPAQUE) && self.is_named_package_object(owner) {
            "top-level opaque type"
        } else if flags.is(f::TRAIT) && flags.is_one_of(f::FINAL.or(f::OPEN)) {
            "final or open trait"
        } else if kind != Kind::Class && flags.is(f::ABSTRACT) {
            "abstract non-class"
        } else if kind == Kind::Val && !flags.is(f::TERM_PARAM) && name.ends_with("_=") {
            "value name ending in _="
        } else if !is_type && flags.is(f::DEFERRED) && !self.is_class_owner(owner) {
            "abstract member outside a class"
        } else if !is_type && flags.is(f::DEFERRED) && flags.is(f::PRIVATE) {
            "private abstract member"
        } else if kind == Kind::Def && flags.is(f::LAZY) {
            "lazy method"
        } else if !is_type && flags.is(f::TRANSPARENT) && !flags.is(f::INLINE) {
            "transparent without inline"
        } else if self.doubly_defined(tree, name, kind, ctx) {
            "double definition"
        } else if self.u.symbol(owner).kind == Kind::Package && self.has_package(owner, name) {
            "definition named like a package"
        } else {
            return None;
        };
        Some(problem)
    }

    /// Checks that no class or type alias of class `cls`, whose parents are set, has the name of
    /// a class it inherits (dotty's `CannotHaveSameNameAs`).
    pub fn check_member_classes(&mut self, cls: SymbolId, rhs: NodeId) {
        let members: Vec<SymbolId> = self.u.symbol(cls).decls.clone();
        for m in members {
            let kind = self.u.symbol(m).kind;
            let deferred = self.syms.flags.get(&m).is_some_and(|fl| fl.is(f::DEFERRED));
            let name = self.u.symbol(m).name.clone();
            if (kind == Kind::Class || kind == Kind::Type && !deferred) && self.inherits_class(cls, &name) {
                return self.unsupported_at(self.ast.span(rhs), "type shadowing an inherited class");
            }
        }
    }

    /// Checks that the bounds or alias `info` of type `sym` do not lead back to `sym` other than
    /// through a class's type arguments (dotty's `checkNonCyclic`).
    pub fn check_non_cyclic(&mut self, sym: SymbolId, info: tasty::TypeId, tree: NodeId) {
        if self.reaches(info, sym, 0) {
            self.unsupported(tree, "cyclic type reference");
        }
    }

    fn reaches(&mut self, t: tasty::TypeId, target: SymbolId, depth: u32) -> bool {
        use tasty::Type;
        if depth > 32 {
            return true;
        }
        let t = self.u.dealias(t);
        match self.u.types().get(t).clone() {
            Type::Sym(_, s) if s == target => true,
            Type::Sym(_, s) if matches!(self.u.symbol(s).kind, Kind::TypeParam | Kind::Type) => {
                self.complete(s);
                let info = self.u.info(s);
                self.reaches(info, target, depth + 1)
            }
            Type::Bounds(a, b) | Type::And(a, b) | Type::Or(a, b, _) => {
                self.reaches(a, target, depth + 1) || self.reaches(b, target, depth + 1)
            }
            Type::Alias(a) | Type::Applied(a, _) => self.reaches(a, target, depth + 1),
            _ => false,
        }
    }

    /// Whether `owner` is a class that may declare abstract members: not an object.
    fn is_class_owner(&self, owner: SymbolId) -> bool {
        let s = self.u.symbol(owner);
        s.kind == Kind::Class && !s.flags.has(tasty::tags::OBJECT)
    }

    /// Whether package `owner` has a subpackage named `name`.
    fn has_package(&mut self, owner: SymbolId, name: &str) -> bool {
        let decls = self.u.decl(owner, name).unwrap_or_default();
        decls.into_iter().any(|d| self.u.symbol(d).kind == Kind::Package)
    }

    /// Whether a base class of class `c` other than `c` has a member class named `name`.
    fn inherits_class(&mut self, c: SymbolId, name: &str) -> bool {
        let bases = self.u.linearization(c);
        bases.iter().skip(1).any(|&b| {
            let decls = &self.u.symbol(b).decls;
            decls.iter().any(|&d| self.u.symbol(d).kind == Kind::Class && self.u.symbol(d).name == name)
        })
    }
}
