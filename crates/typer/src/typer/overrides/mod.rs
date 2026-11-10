// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/RefChecks.scala
//   compiler/src/dotty/tools/dotc/transform/ElimErasedValueType.scala
//   compiler/src/dotty/tools/dotc/core/SymDenotations.scala

mod abstract_members;
mod inherited_conflicts;
mod member_overrides;
mod name_clashes;
mod override_kinds;
mod overrides_nothing;

use super::Typer;
use crate::symbols::Syms;
use syntax::ast::Span;
use syntax::ast::flags as f;
use tasty::tags::{ABSTRACT, TRAIT};
use tasty::{Kind, SymbolId, TypeId, Universe};

/// The override checks of one class (dotty's `RefChecks.checkAllOverrides` with
/// `checkNoAbstractMembers` and `ElimErasedValueType.checkNoClashes`): refuses the class at its
/// span when two of its methods clash, a member overrides another it may not or without
/// `override`, an `override` member overrides nothing, it inherits conflicting members, or it is
/// concrete with an abstract member left unimplemented. A class with a macro annotation keeps its
/// last check until the annotations expanded.
pub struct OverrideChecks {
    cls: SymbolId,
    span: Span,
    /// The class's type, as a prefix to see members from.
    site: TypeId,
    /// The base classes of the class, nearest first, without the class.
    bases: Vec<SymbolId>,
}

impl Typer<'_> {
    /// Refuses class `cls`, defined at `span`, if dotty's override checks reject it, as for
    /// `class A { def f = 1 }; class B extends A { def f = 2 }` without `override`.
    pub fn check_overrides(&mut self, cls: SymbolId, span: Span) {
        let site = crate::defn::type_ref(self.u, cls);
        let bases = self.u.linearization(cls).into_iter().skip(1).collect();
        OverrideChecks { cls, span, site, bases }.run(self);
    }

    /// Checks the classes with macro annotations on them or their declarations for unimplemented
    /// members, as dotty's `RefChecks` does after `MacroAnnotations`.
    pub fn check_macro_annotated(&mut self) {
        for checks in std::mem::take(&mut self.macro_annotated) {
            if let Some(what) = checks.unimplemented(self, true) {
                self.unsupported_at(checks.span, what);
            }
        }
    }
}

impl OverrideChecks {
    /// Runs the checks in dotty's order, refusing at the first failing one; a name clash and
    /// inherited conflicts do not stop the later checks.
    fn run(self, t: &mut Typer<'_>) {
        let own = t.u.symbol(self.cls).decls.clone();
        if let Some(what) = self.double_definition(t, &own) {
            return t.unsupported_at(self.span, what);
        }
        if let Some(what) = self.name_clash(t) {
            t.unsupported_at(self.span, what);
        }
        if let Some(what) = self.member_refusal(t, own) {
            return t.unsupported_at(self.span, what);
        }
        if self.overrides_nothing(t) {
            return t.unsupported_at(self.span, "member overriding nothing");
        }
        if self.inherited_conflict(t) {
            t.unsupported_at(self.span, "conflicting inherited members");
        }
        let class = t.u.symbol(self.cls);
        if class.flags.has(ABSTRACT) || class.flags.has(TRAIT) {
            return;
        }
        if t.macros.is_some() && has_macro_annotation(t.u, self.cls) {
            return t.macro_annotated.push(self);
        }
        if let Some(what) = self.unimplemented(t, false) {
            t.unsupported_at(self.span, what);
        }
    }

    /// Why two methods of `own`, the class's declarations, may not be defined together, if they
    /// may not: they match with the same erasure, or both have default arguments.
    fn double_definition(&self, t: &mut Typer<'_>, own: &[SymbolId]) -> Option<&'static str> {
        let defs: Vec<SymbolId> = own.iter().copied().filter(|&d| t.u.symbol(d).kind == Kind::Def).collect();
        for (k, &a) in defs.iter().enumerate() {
            for &b in &defs[..k] {
                if t.u.symbol(a).name != t.u.symbol(b).name {
                    continue;
                }
                if t.same_member(self.site, a, b) && t.u.erased_method(a).0 == t.u.erased_method(b).0 {
                    return Some("double definition");
                }
                if has_default_args(&t.syms, a) && has_default_args(&t.syms, b) {
                    return Some("overloads with default arguments");
                }
            }
        }
        None
    }
}

/// Whether method `def` of the unit has a parameter with a default argument.
fn has_default_args(syms: &Syms, def: SymbolId) -> bool {
    let clauses = syms.param_clauses.get(&def);
    clauses.into_iter().flatten().flatten().any(|p| syms.flags.get(p).is_some_and(|fl| fl.is(f::HAS_DEFAULT)))
}

/// The term members of class `c` named `name` that a subclass can override.
fn overridable(u: &Universe, c: SymbolId, name: &str) -> Vec<SymbolId> {
    let keep = |&&d: &&SymbolId| {
        let s = u.symbol(d);
        s.name == name && s.is_term_member() && !s.is_private()
    };
    u.symbol(c).decls.iter().filter(keep).copied().collect()
}

/// Whether `cls` or one of its declarations has a macro annotation.
fn has_macro_annotation(u: &mut Universe, cls: SymbolId) -> bool {
    let base = u.class_ref("scala.annotation.MacroAnnotation");
    let Some(base) = u.class_symbol(base) else {
        return false;
    };
    let mut annotated = u.symbol(cls).decls.clone();
    annotated.push(cls);
    let annotations: Vec<_> = annotated.into_iter().flat_map(|s| u.annotations(s).to_vec()).collect();
    annotations.into_iter().any(|a| u.derives_from(a, base))
}
