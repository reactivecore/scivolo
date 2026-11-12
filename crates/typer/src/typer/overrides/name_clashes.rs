// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/transform/ElimErasedValueType.scala
//! Members with the same run-time name and erased signature (dotty's
//! `ElimErasedValueType.checkNoClashes`).

use super::super::Typer;
use super::OverrideChecks;
use crate::symbols::{Completion, Syms};
use names::nme;
use syntax::ast::{Ast, Tree};
use tasty::tags::{ERASED, INLINE, MACRO, PRIVATE};
use tasty::{Kind, SymbolId, Universe};

impl OverrideChecks {
    /// Why two members of the class, or one of them and an inherited member of another name or
    /// of a type it does not match, may not be defined together, if they may not: they have the
    /// same name at run time and the same erased signature, as `def f(x: List[Int])` and
    /// `def f(x: List[String])`.
    pub fn name_clash(&self, t: &mut Typer<'_>) -> Option<&'static str> {
        let decls = t.u.symbol(self.cls).decls.clone();
        let own: Vec<SymbolId> = decls.into_iter().filter(|&d| at_run_time(t.u, d)).collect();
        own.iter().for_each(|&d| t.complete_erased_classes(d));
        for (k, &a) in own.iter().enumerate() {
            for &b in &own[..k] {
                if t.u.symbol(a).run_name() == t.u.symbol(b).run_name()
                    && t.is_scala_static(a) == t.is_scala_static(b)
                    && t.u.erased_method(a) == t.u.erased_method(b)
                {
                    return Some("double definition");
                }
            }
        }
        for &b in &self.bases {
            let inherited = t.u.symbol(b).decls.clone();
            for m in inherited {
                if has_pending_annotations(&t.syms, t.ast, m) {
                    t.complete(m);
                }
                if !at_run_time(t.u, m) {
                    continue;
                }
                for &d in &own {
                    let same_name = t.u.symbol(m).name == t.u.symbol(d).name;
                    let defs = [m, d].iter().all(|&s| t.u.symbol(s).kind == Kind::Def);
                    let loose = !defs || b == t.u.defn().any || t.is_java_member(m);
                    if same_name && loose || t.u.symbol(m).run_name() != t.u.symbol(d).run_name() {
                        continue;
                    }
                    t.complete(m);
                    t.complete_erased_classes(m);
                    if t.u.erased_method(m) == t.u.erased_method(d)
                        && (!same_name || !t.u.info_matches(self.site, d, m))
                    {
                        return Some("name clash between defined and inherited member");
                    }
                }
            }
        }
        None
    }
}

/// Whether script symbol `s` is not completed yet and its definition has annotations, one of
/// which may give it a run-time name.
fn has_pending_annotations(syms: &Syms, ast: &Ast, s: SymbolId) -> bool {
    let Some(&Completion::Pending { tree, .. }) = syms.completions.get(&s) else { return false };
    let (Tree::ValDef { mods, .. } | Tree::DefDef { mods, .. }) = *ast.tree(tree) else {
        return false;
    };
    !ast.items(ast.modifiers(mods).annotations).is_empty()
}

/// Whether member `s` is a method or value with a run-time name in its class.
fn at_run_time(u: &Universe, s: SymbolId) -> bool {
    let sym = u.symbol(s);
    let absent = [PRIVATE, INLINE, ERASED, MACRO].iter().any(|&f| sym.flags.has(f));
    matches!(sym.kind, Kind::Def | Kind::Val) && sym.name != nme::CONSTRUCTOR && !absent
}
