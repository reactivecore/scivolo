// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/transform/MacroAnnotations.scala
//   compiler/src/dotty/tools/dotc/transform/Inlining.scala
//! Macro annotations: each definition of a statement list that an annotation extending
//! `scala.annotation.MacroAnnotation` is on, replaced by the definitions its `transform` returns,
//! and its companion by the one returned (dotty's `MacroAnnotations.expandAnnotations`).

use super::macro_definitions::is_definition;
use super::macros::{AnnotationCall, Expanded, MacroError};
use super::{Inlining, Unsupported};
use crate::tpd::{TNodeId, Tree as T};
use rustc_hash::FxHashMap;
use syntax::ast::Span;
use tasty::tags::OBJECT;
use tasty::{SymbolId, Type};

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
        let mut stats = self.tpd.items(stats).to_vec();
        let (mut i, mut changed) = (0, false);
        while let Some(&stat) = stats.get(i) {
            let annotations = self.macro_annotations(stat)?;
            if annotations.is_empty() {
                i += 1;
                continue;
            }
            changed = true;
            let companion = self.companion_class_of(stat);
            let at = companion.and_then(|c| stats.iter().position(|&d| self.is_class_def(d, c)));
            let (defs, new_companion) = self.expand_annotations(stat, &annotations, at.map(|j| stats[j]))?;
            if let (Some(j), Some(c)) = (at, new_companion) {
                stats[j] = c;
            }
            let n = defs.len();
            stats.splice(i..=i, defs);
            i += n;
        }
        if changed {
            let new = self.tpd.list(&stats);
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
    /// one after the other, and the companion `companion` turns into if it changed.
    fn expand_annotations(
        &mut self,
        def: TNodeId,
        annotations: &[TNodeId],
        mut companion: Option<TNodeId>,
    ) -> Result<(Vec<TNodeId>, Option<TNodeId>), Unsupported> {
        let span = self.tpd.span(def);
        let sym = self.tpd.sym(def).ok_or((span, "a macro annotation on a definition without a symbol"))?;
        let owner =
            self.u.symbol(sym).owner.ok_or((span, "a macro annotation on a definition without an owner"))?;
        let companion_sym = companion.and_then(|c| self.tpd.sym(c));
        let mut defs = vec![def];
        for &annotation in annotations {
            let at =
                defs.iter().position(|&d| self.tpd.sym(d) == Some(sym)).expect("the annotated definition");
            let call = AnnotationCall { annotation, definition: defs[at], companion, owner, span };
            let fresh = self.u.next_symbol();
            let lent = (&mut *self.u, &mut *self.tpd, &mut *self.names, &mut *self.fresh);
            let expanded = self.host.expand_annotation(lent, &call);
            let Some(expanded) = self.expanded_or_reported(expanded, span)? else {
                return Ok((defs, companion));
            };
            let mut made = Vec::new();
            for tree in self.quoted_trees(&expanded, &[])? {
                self.check_enclosing_this(tree, owner)?;
                self.own_right_hand_sides(tree);
                let tree = self.retype(tree, &FxHashMap::default(), owner)?;
                self.enter_made(tree, owner, span)?;
                made.push(tree);
            }
            self.check_made(&made, (defs[at], companion), fresh, span)?;
            if let Some(c) = companion_sym {
                let (mut found, rest): (Vec<_>, Vec<_>) =
                    made.into_iter().partition(|&d| self.tpd.sym(d) == Some(c));
                match found.len() {
                    0 => {}
                    1 => companion = found.pop(),
                    _ => return Err((span, "a macro annotation returning its companion twice")),
                }
                made = rest;
            }
            if !made.iter().any(|&d| self.tpd.sym(d) == Some(sym)) {
                return Err((span, "a macro annotation that drops the definition it is on"));
            }
            defs.splice(at..=at, made);
        }
        Ok((defs, companion))
    }

    /// The expansion of `expanded`, or `None` with its errors reported.
    fn expanded_or_reported(
        &mut self,
        expanded: Result<Expanded, MacroError>,
        span: Span,
    ) -> Result<Option<Expanded>, Unsupported> {
        match expanded {
            Ok(expanded) => return Ok(Some(expanded)),
            Err(MacroError::Unsupported(_) | MacroError::Search(_)) => {
                return Err((span, "a macro annotation the frontend refuses"));
            }
            Err(MacroError::Failed(msg)) => self.error(span, msg),
            Err(MacroError::Reported(errors)) => errors.into_iter().for_each(|(at, msg)| self.error(at, msg)),
            Err(MacroError::Aborted) if !self.host.has_errors() => self.error(span, ABORTED.to_owned()),
            Err(MacroError::Aborted) => {}
        }
        Ok(None)
    }

    /// The class of the companion of class definition `def`, if it has one.
    fn companion_class_of(&mut self, def: TNodeId) -> Option<SymbolId> {
        let s = self.tpd.sym(def).filter(|&s| self.u.is_class(s))?;
        match self.u.is_module_class(s) {
            true => self.u.companion_class(s),
            false => self.u.companion_module(s).and_then(|m| self.u.module_class(m)),
        }
    }

    fn is_class_def(&self, d: TNodeId, class: SymbolId) -> bool {
        matches!(self.tpd.tree(d), T::TypeDef { .. }) && self.tpd.sym(d) == Some(class)
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
        let mut trees = self.host.annotations(s);
        trees.retain(|&t| self.u.class_symbol(self.tpd.tpe(t)).is_some_and(|c| self.u.derives_from(c, base)));
        let typed: Vec<_> = trees.iter().map(|&t| self.u.class_symbol(self.tpd.tpe(t))).collect();
        let classes = self.u.annotations(s).to_vec();
        if classes.into_iter().any(|c| self.u.derives_from(c, base) && !typed.contains(&Some(c))) {
            return Err((self.tpd.span(def), "a macro annotation in a quote or a library inline body"));
        }
        trees.reverse();
        Ok(trees)
    }
}
