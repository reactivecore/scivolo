//! Members with the same run-time name and erased signature (dotty's
//! `ElimErasedValueType.checkNoClashes`).

use super::Typer;
use crate::symbols::Completion;
use syntax::ast::{Span, Tree};
use tasty::tags::{ERASED, INLINE, MACRO, PRIVATE};
use tasty::{Kind, SymbolId};

impl Typer<'_> {
    /// Refuses class `cls` if two of its members, or one of them and an inherited member of
    /// another name, have the same name at run time and the same erased signature.
    pub fn check_no_clashes(&mut self, cls: SymbolId, span: Span) {
        let decls = self.u.symbol(cls).decls.clone();
        let own: Vec<SymbolId> = decls.into_iter().filter(|&d| self.at_run_time(d)).collect();
        for (k, &a) in own.iter().enumerate() {
            for &b in &own[..k] {
                if self.run_name(a) == self.run_name(b) && self.u.erased_method(a) == self.u.erased_method(b)
                {
                    return self.unsupported_at(span, "double definition");
                }
            }
        }
        let bases: Vec<SymbolId> = self.u.linearization(cls).into_iter().skip(1).collect();
        for b in bases {
            let inherited = self.u.symbol(b).decls.clone();
            for m in inherited {
                if self.has_pending_annotations(m) {
                    self.complete(m);
                }
                if !self.at_run_time(m) {
                    continue;
                }
                for &d in &own {
                    let same_name = self.u.symbol(m).name == self.u.symbol(d).name;
                    if same_name || self.run_name(m) != self.run_name(d) {
                        continue;
                    }
                    self.complete(m);
                    if self.u.erased_method(m) == self.u.erased_method(d) {
                        return self.unsupported_at(span, "name clash between defined and inherited member");
                    }
                }
            }
        }
    }

    /// Whether script symbol `s` is not completed yet and its definition has annotations, one of
    /// which may give it a run-time name.
    fn has_pending_annotations(&self, s: SymbolId) -> bool {
        let Some(&Completion::Pending { tree, .. }) = self.syms.completions.get(&s) else { return false };
        let (Tree::ValDef { mods, .. } | Tree::DefDef { mods, .. }) = *self.ast.tree(tree) else {
            return false;
        };
        !self.ast.items(self.ast.modifiers(mods).annotations).is_empty()
    }

    /// Whether member `s` is a method or value with a run-time name in its class.
    fn at_run_time(&self, s: SymbolId) -> bool {
        let sym = self.u.symbol(s);
        let absent = [PRIVATE, INLINE, ERASED, MACRO].iter().any(|&f| sym.flags.has(f));
        matches!(sym.kind, Kind::Def | Kind::Val) && sym.name != "<init>" && !absent
    }
}
