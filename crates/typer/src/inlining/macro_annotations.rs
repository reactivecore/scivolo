// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/transform/MacroAnnotations.scala
//! Macro annotations: each definition of a statement list that an annotation extending
//! `scala.annotation.MacroAnnotation` is on, replaced by the definitions its `transform` returns
//! (dotty's `MacroAnnotations.expandAnnotations`).

use super::macros::{AnnotationCall, MacroError};
use super::{Inlining, Unsupported};
use crate::tpd::{TNodeId, Tree as T};
use rustc_hash::FxHashMap;
use tasty::tags::OBJECT;
use tasty::{Kind, Type};

const MACRO_ANNOTATION: &str = "scala.annotation.MacroAnnotation";
const ABORTED: &str = "Macro expansion was aborted by the macro without any errors reported. Macros should issue \
                       errors to end-users when aborting a macro expansion with StopMacroExpansion.";

impl Inlining<'_> {
    /// Expands the macro annotations on the definitions among the statements of `t`, a template,
    /// block or package.
    pub fn expand_macro_annotations(&mut self, t: TNodeId) -> Result<(), Unsupported> {
        let stats = match *self.tpd.tree(t) {
            T::Template { body: stats, .. } | T::Block { stats, .. } | T::PackageDef { stats, .. } => stats,
            _ => return Ok(()),
        };
        let items = self.tpd.items(stats).to_vec();
        let mut out = Vec::with_capacity(items.len());
        let mut changed = false;
        for stat in items {
            let annotations = self.macro_annotations(stat)?;
            if annotations.is_empty() {
                out.push(stat);
                continue;
            }
            changed = true;
            out.extend(self.expand_annotations(stat, &annotations)?);
        }
        if changed {
            let new = self.tpd.list(&out);
            match &mut self.tpd.get_mut(t).tree {
                T::Template { body: stats, .. } | T::Block { stats, .. } | T::PackageDef { stats, .. } => {
                    *stats = new
                }
                _ => unreachable!("a tree with statements"),
            }
        }
        Ok(())
    }

    /// The definitions that macro annotations `annotations` on definition `def` turn it into,
    /// one after the other.
    fn expand_annotations(
        &mut self,
        def: TNodeId,
        annotations: &[TNodeId],
    ) -> Result<Vec<TNodeId>, Unsupported> {
        let span = self.tpd.span(def);
        let sym = self.tpd.sym(def).ok_or((span, "a macro annotation on a definition without a symbol"))?;
        let owner =
            self.u.symbol(sym).owner.ok_or((span, "a macro annotation on a definition without an owner"))?;
        if matches!(self.tpd.tree(def), T::TypeDef { .. }) {
            return Err((span, "a macro annotation on a class"));
        }
        let mut defs = vec![def];
        for &annotation in annotations {
            let at =
                defs.iter().position(|&d| self.tpd.sym(d) == Some(sym)).expect("the annotated definition");
            let call = AnnotationCall { annotation, definition: defs[at], companion: None, owner, span };
            let lent = (&mut *self.u, &mut *self.tpd, &mut *self.names, &mut *self.fresh);
            let expanded = match self.host.expand_annotation(lent, &call) {
                Ok(expanded) => expanded,
                Err(MacroError::Unsupported(_) | MacroError::Search(_)) => {
                    return Err((span, "a macro annotation the frontend refuses"));
                }
                Err(MacroError::Failed(msg)) => {
                    self.error(span, msg);
                    return Ok(defs);
                }
                Err(MacroError::Reported(errors)) => {
                    for (at, msg) in errors {
                        self.error(at, msg);
                    }
                    return Ok(defs);
                }
                Err(MacroError::Aborted) => {
                    if !self.host.has_errors() {
                        self.error(span, ABORTED.to_owned());
                    }
                    return Ok(defs);
                }
            };
            let mut made = Vec::new();
            for tree in self.quoted_trees(&expanded, &[])? {
                if let T::ValDef { rhs: Some(rhs), .. } | T::DefDef { rhs: Some(rhs), .. } =
                    *self.tpd.tree(tree)
                    && let Some(s) = self.tpd.sym(tree)
                {
                    self.reown(rhs, s);
                }
                made.push(self.retype(tree, &FxHashMap::default(), owner)?);
            }
            if !made.iter().any(|&d| self.tpd.sym(d) == Some(sym)) {
                return Err((span, "a macro annotation that drops the definition it is on"));
            }
            for &d in &made {
                let s = self.tpd.sym(d).filter(|_| is_definition(self.tpd.tree(d)));
                let s = s.ok_or((span, "a macro annotation returning other than definitions"))?;
                if self.u.symbol(s).owner != Some(owner) {
                    return Err((span, "a macro annotation adding a definition of another owner"));
                }
                if self.u.symbol(owner).kind == Kind::Class && !self.u.symbol(owner).decls.contains(&s) {
                    self.u.enter(s);
                }
            }
            defs.splice(at..=at, made);
        }
        Ok(defs)
    }

    /// The typed macro annotations on definition `def`, last first as dotty keeps them; none on
    /// a module value, whose class carries them (dotty's `hasMacroAnnotation`).
    fn macro_annotations(&mut self, def: TNodeId) -> Result<Vec<TNodeId>, Unsupported> {
        let Some(s) = self.tpd.sym(def).filter(|_| is_definition(self.tpd.tree(def))) else {
            return Ok(Vec::new());
        };
        if self.u.annotations(s).is_empty()
            || self.u.symbol(s).kind.is_term() && self.u.symbol(s).flags.has(OBJECT)
        {
            return Ok(Vec::new());
        }
        let base = self.u.class_ref(MACRO_ANNOTATION);
        let Type::Sym(_, base) = *self.u.types().get(base) else { return Ok(Vec::new()) };
        let classes = self.u.annotations(s).to_vec();
        let classes = classes.into_iter().filter(|&c| self.u.derives_from(c, base)).count();
        let mut trees = self.host.annotations(s);
        trees.retain(|&t| self.u.class_symbol(self.tpd.tpe(t)).is_some_and(|c| self.u.derives_from(c, base)));
        if classes > 0 && trees.is_empty() {
            return Err((self.tpd.span(def), "a macro annotation in a quote or a library inline body"));
        }
        trees.reverse();
        Ok(trees)
    }
}

fn is_definition(t: &T) -> bool {
    matches!(t, T::ValDef { .. } | T::DefDef { .. } | T::TypeDef { .. })
}
