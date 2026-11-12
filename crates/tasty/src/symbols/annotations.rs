//! The annotations of symbols: recorded for those defined outside TASTy, read from TASTy for
//! the others.

use super::entries::{annotations, first_int, first_string, names_ref};
use super::{FileId, SymbolId, Universe};
use crate::{NodeId, View, view};

impl Universe {
    /// Whether an annotation of the class with full name `full` annotates `sym`.
    pub fn is_annotated(&mut self, sym: SymbolId, full: &str) -> bool {
        if self.symbol(sym).tree.is_none() {
            return self.has_recorded_annotation(sym, full);
        }
        self.annotation_call(sym, full).is_some()
    }

    /// Whether an annotation class recorded for `sym` has full name `full`.
    pub fn has_recorded_annotation(&self, sym: SymbolId, full: &str) -> bool {
        self.annotations(sym).iter().any(|&c| self.full_name(c) == full)
    }

    /// Records one more annotation with arguments of class `class` on `sym`, a symbol defined
    /// outside TASTy, with `arg` if a string literal is its only argument.
    pub fn annotate_string(&mut self, sym: SymbolId, class: SymbolId, arg: Option<String>) {
        self.annotation_strings.entry((sym, class)).or_default().push(arg);
    }

    /// The string literal arguments recorded for the annotations of class `class` on `sym`, one
    /// per annotation with arguments, in order.
    pub fn annotation_strings(&self, sym: SymbolId, class: SymbolId) -> &[Option<String>] {
        self.annotation_strings.get(&(sym, class)).map_or(&[], Vec::as_slice)
    }

    /// Records meta-annotation class `meta` on the annotation of class `class` on `sym`.
    pub fn annotate_meta(&mut self, sym: SymbolId, class: SymbolId, meta: SymbolId) {
        self.annotation_metas.entry((sym, class)).or_default().push(meta);
    }

    /// The annotation classes of `sym` meta-annotated `scala.annotation.meta.<target>` for one
    /// of `targets`, where they annotate it or their class, and if `none_of` is not empty, those
    /// meta-annotated for none of `none_of` (dotty's `annotationsCarrying`).
    pub fn annotations_carrying(
        &mut self,
        sym: SymbolId,
        targets: &[&str],
        none_of: &[&str],
    ) -> Vec<SymbolId> {
        let mut classes = self.annotations(sym).to_vec();
        classes.retain(|&c| {
            let mut metas = self.annotation_metas.get(&(sym, c)).cloned().unwrap_or_default();
            metas.extend(self.tasty_annotations(c));
            metas.extend_from_slice(self.annotations(c));
            metas.retain(|&m| self.full_name(m).starts_with("scala.annotation.meta."));
            let carries = |set: &[&str]| metas.iter().any(|&m| set.contains(&self.symbol(m).name.as_str()));
            carries(targets) || !none_of.is_empty() && !carries(none_of)
        });
        classes
    }

    /// The first `Int` argument of the annotation of the class with full name `full` on method
    /// `sym` in its TASTy.
    pub fn annotation_int(&mut self, sym: SymbolId, full: &str) -> Option<i32> {
        let (file, call) = self.annotation_call(sym, full)?;
        first_int(self.file(file).0, call)
    }

    /// The first string argument of the annotation of the class with full name `full` on method
    /// `sym` in its TASTy.
    pub fn annotation_string(&mut self, sym: SymbolId, full: &str) -> Option<String> {
        let (file, call) = self.annotation_call(sym, full)?;
        let (ast, names) = self.file(file);
        first_string(ast, names, call)
    }

    /// Records the annotations on `from`, in its TASTy or recorded, on `to` with their string
    /// arguments and meta-annotations, but those of the class with full name `except`.
    pub fn copy_annotations(&mut self, from: SymbolId, to: SymbolId, except: &str) {
        let classes = match self.symbol(from).tree {
            Some(_) => self.tasty_annotations(from),
            None => self.annotations(from).to_vec(),
        };
        for class in classes {
            if self.full_name(class) != except {
                self.copy_annotation(from, to, class);
            }
        }
    }

    /// Records the annotations of class `class` on `from` on `to` with their string arguments
    /// and meta-annotations.
    pub fn copy_annotation(&mut self, from: SymbolId, to: SymbolId, class: SymbolId) {
        self.annotate(to, class);
        for arg in self.annotation_strings(from, class).to_vec() {
            self.annotate_string(to, class, arg);
        }
        if let Some(metas) = self.annotation_metas.get(&(from, class)).cloned() {
            self.annotation_metas.entry((to, class)).or_default().extend(metas);
        }
    }

    /// The classes of the annotations on `sym` in its TASTy, in order.
    pub fn tasty_annotations(&mut self, sym: SymbolId) -> Vec<SymbolId> {
        let Some((file, id)) = self.symbol(sym).tree else { return Vec::new() };
        let (ast, _) = self.file(file);
        let mods = match view(ast, id) {
            View::DefDef { parts, .. } => parts.mods,
            View::ValDef { mods, .. } | View::Param { mods, .. } | View::TypeDef { mods, .. } => mods,
            _ => return Vec::new(),
        };
        let types: Vec<NodeId> = annotations(ast, mods).map(|(ty, _)| ty).collect();
        let types: Vec<_> = types.into_iter().map(|ty| self.type_at(file, ty)).collect();
        types.into_iter().filter_map(|t| self.class_symbol(t)).collect()
    }

    /// The constructor call of the first annotation of the class with full name `full` on method
    /// or class `sym` in its TASTy.
    fn annotation_call(&mut self, sym: SymbolId, full: &str) -> Option<(FileId, NodeId)> {
        let (file, id) = self.symbol(sym).tree?;
        let name = full.rsplit('.').next().unwrap_or(full);
        let (ast, names) = self.file(file);
        let mods = match view(ast, id) {
            View::DefDef { parts, .. } => parts.mods,
            View::ValDef { mods, .. } | View::Param { mods, .. } | View::TypeDef { mods, .. } => mods,
            _ => return None,
        };
        let named: Vec<_> =
            annotations(ast, mods).filter(|&(ty, _)| names_ref(ast, names, ty, name)).collect();
        named.into_iter().find_map(|(ty, call)| {
            let t = self.type_at(file, ty);
            self.is_class_named(t, full).then_some((file, call))
        })
    }
}
