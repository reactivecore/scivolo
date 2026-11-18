// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Checking.scala
//! Definitions outside the subset, or that dotty's checks (`Checking`, `RefChecks`) reject and the
//! typer does not report yet: both fall back.

use crate::ctx::Ctx;
use crate::typer::Typer;
use syntax::ast::NodeId;
use syntax::ast::flags::{self as f, FlagSet};
use tasty::{Kind, SymbolId, Type, TypeId};

/// The full name of the class of arrays.
const ARRAY: &str = "scala.Array";

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
        } else if kind == Kind::Type && flags.is_all_of(f::PRIVATE.or(f::OPAQUE)) {
            "illegal combination of modifiers"
        } else if flags.is_all_of(f::DEFERRED_GIVEN_FLAGS) && !self.u.is_trait(owner) {
            "deferred given"
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
        } else if !self.is_scala2_macro_in_scala3(owner, flags) && self.doubly_defined(tree, name, kind, ctx)
        {
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
            let flags = self.syms.flags.get(&m).copied().unwrap_or(f::EMPTY);
            if flags.is(f::PRIVATE) || flags.is(f::DEFERRED) {
                continue;
            }
            let name = self.u.symbol(m).name.clone();
            if matches!(kind, Kind::Class | Kind::Type) && self.inherits_class(cls, &name) {
                return self.unsupported_at(self.ast.span(rhs), "type shadowing an inherited class");
            }
        }
    }

    /// Refuses type parameter `sym` of bounds `info` if its upper bound is an array of itself, as
    /// `F[x] <: Array[F[x]]` or `T <: Array[T] | Null`, which dotty reports as exceeding its
    /// recursion limit. A type member is refused only when its erasure is needed
    /// (`erasure_exhausted`).
    pub fn check_array_self_bound(&mut self, sym: SymbolId, info: TypeId, tree: NodeId) {
        if self.u.symbol(sym).kind != Kind::TypeParam {
            return;
        }
        let info = self.lambda_body(info);
        let Some((_, hi)) = self.u.bounds(info) else { return };
        let hi = self.lambda_body(hi);
        if self.is_array_of(hi, sym) {
            self.unsupported(tree, tasty::ARRAY_OF_ITSELF);
        }
    }

    /// The body of type lambda `t`, else `t`.
    fn lambda_body(&self, t: TypeId) -> TypeId {
        match *self.u.ty(t) {
            Type::Lambda { result, .. } => result,
            _ => t,
        }
    }

    /// Whether `t` is an array, possibly nested or in a union or intersection, of `sym` or of an
    /// application of it.
    fn is_array_of(&mut self, t: TypeId, sym: SymbolId) -> bool {
        let t = self.u.dealias(t);
        if let Type::And(a, b) | Type::Or(a, b, _) = *self.u.ty(t) {
            return self.is_array_of(a, sym) || self.is_array_of(b, sym);
        }
        let Some((tycon, args)) = self.u.applied(t) else { return false };
        let [elem] = *args else { return false };
        if self.u.class_symbol(tycon).is_none_or(|c| self.u.full_name(c) != ARRAY) {
            return false;
        }
        let head = self.u.applied(elem).map_or(elem, |(c, _)| c);
        self.u.sym_of(head) == Some(sym) || self.is_array_of(elem, sym)
    }

    /// Whether `owner` is a class that may declare abstract members: not an object.
    fn is_class_owner(&self, owner: SymbolId) -> bool {
        let s = self.u.symbol(owner);
        s.kind == Kind::Class && !s.flags.has(tasty::tags::OBJECT)
    }

    /// Whether package `owner` has a subpackage named `name`.
    fn has_package(&mut self, owner: SymbolId, name: &str) -> bool {
        let decls = self.u.decls_named(owner, name);
        decls.into_iter().any(|d| self.u.symbol(d).kind == Kind::Package)
    }

    /// Whether a base class of class `c` other than `c` has a non-private member class named `name`.
    fn inherits_class(&mut self, c: SymbolId, name: &str) -> bool {
        let bases = self.u.linearization(c);
        bases.iter().skip(1).any(|&b| {
            let decls = &self.u.symbol(b).decls;
            decls.iter().any(|&d| {
                let d = self.u.symbol(d);
                d.kind == Kind::Class && d.name == name && !d.flags.has(tasty::tags::PRIVATE)
            })
        })
    }
}
