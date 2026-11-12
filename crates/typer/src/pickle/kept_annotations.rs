// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/transform/PostTyper.scala
//   compiler/src/dotty/tools/dotc/core/tasty/TreePickler.scala
//! The annotations a definition keeps, in order (`PostTyper`'s `processValOrDefDef`), and the
//! parts of them pickled anew.

use super::TreePickler;
use crate::tpd::{TNodeId, Tree as T};
use names::name_kinds::SETTER_SUFFIX;
use tasty::defn::{NON_BEAN_TARGETS, UNUSED};
use tasty::tags::{FIELDaccessor, PARAMsetter, SYNTHETIC};
use tasty::{Kind, Symbol, SymbolId};

/// The meta-annotation target of parameters, `scala.annotation.meta.param`.
const PARAM: &[&str] = &["param"];

/// The meta-annotation target of setters, `scala.annotation.meta.setter`.
const SETTER: &[&str] = &["setter"];

/// The meta-annotation targets of getters and fields.
const GETTER_FIELD: &[&str] = &["getter", "field"];

/// The annotations a parameter accessor keeps without a meta-annotation (dotty's
/// `NonBeanParamAccessorAnnots`).
const PARAM_ACCESSOR_ALSO: [&str; 3] =
    ["scala.annotation.publicInBinary", "scala.caps.use", "scala.caps.internal.consume"];

/// Whether `s` is the setter of a `var`.
fn is_setter(s: &Symbol) -> bool {
    s.flags.has(FIELDaccessor) && s.name.ends_with(SETTER_SUFFIX)
}

impl TreePickler<'_> {
    /// The annotation trees of `s`, in order; in a file, the ones of its definition last first
    /// (dotty's `addAnnotation` prepends), and on a value parameter its `@unused` first
    /// (`PostTyper`'s `transform` of a parameter), and of a parameter, setter or non-synthetic
    /// `val` only those meant for it (`keepAnnotationsCarrying`): of a `val` or `var` those for
    /// getters or fields, of a class parameter's `val` also `@publicInBinary`, `@use` and
    /// `@consume`, else also those without a target.
    pub fn annotation_trees(&mut self, s: SymbolId) -> Vec<TNodeId> {
        let own = self.tpd.annotations(s).to_vec();
        let Some(file) = self.file else { return own };
        let mut trees = file.annotations.get(&s).cloned().unwrap_or_else(|| own.clone());
        let at: Vec<usize> = (0..trees.len()).filter(|&i| own.contains(&trees[i])).collect();
        for (&i, &a) in at.iter().zip(own.iter().rev()) {
            trees[i] = a;
        }

        let symbol = self.u.symbol(s);
        let (param, accessor) = (symbol.kind == Kind::Param, symbol.flags.has(PARAMsetter));
        let (targets, none_of) = match symbol.kind {
            _ if is_setter(symbol) => (SETTER, &[][..]),
            Kind::Val if symbol.flags.has(SYNTHETIC) => return trees,
            Kind::Val if accessor => (GETTER_FIELD, &[][..]),
            Kind::Val => (GETTER_FIELD, NON_BEAN_TARGETS),
            Kind::Param => (PARAM, NON_BEAN_TARGETS),
            _ => return trees,
        };
        let kept = self.u.annotations_carrying(s, targets, none_of);
        trees.retain(|&a| {
            let t = self.tpd.tpe(a);
            self.u.class_symbol(t).is_some_and(|c| kept.contains(&c))
                || param && self.u.is_class_named(t, UNUSED)
                || accessor && !param && PARAM_ACCESSOR_ALSO.iter().any(|&n| self.u.is_class_named(t, n))
        });
        if !param {
            return trees;
        }
        if let Some(i) = trees.iter().position(|&a| self.u.is_class_named(self.tpd.tpe(a), UNUSED)) {
            trees[..=i].rotate_right(1);
        }
        trees
    }

    /// Marks parts of annotation `annot` of `sym` to be pickled anew if pickled before, its other
    /// parts shared: of a parameter its calls and selections (a field's annotation its
    /// constructor parameter has too), and of a parameter or setter with a meta-annotated class
    /// those, the `new` and its annotated type trees (one its getter has too).
    pub fn annotation_anew(&mut self, sym: SymbolId, annot: TNodeId) {
        if self.buf.addr_of_tree(self.tree_key(annot)).is_none() {
            return;
        }
        let symbol = self.u.symbol(sym);
        let setter = is_setter(symbol);
        if symbol.kind != Kind::Param && !setter {
            return;
        }

        let mut parts = vec![annot];
        let mut t = annot;
        while let T::Apply { fun, .. } | T::TypeApply { fun, .. } = *self.tpd.tree(t) {
            t = fun;
            parts.push(t);
        }
        match self.meta_annotated_new(t) {
            Some(new) => parts.extend(new),
            None if setter => return,
            None => {}
        }
        self.anew.extend(parts.into_iter().map(|p| (p, sym)));
    }

    /// The `new` of selection `select` and its annotated type trees, outermost first, if its type
    /// tree is annotated.
    fn meta_annotated_new(&self, select: TNodeId) -> Option<Vec<TNodeId>> {
        let T::Select { qualifier, .. } = *self.tpd.tree(select) else { return None };
        let T::New { mut tpt } = *self.tpd.tree(qualifier) else { return None };
        let mut parts = vec![qualifier];
        while let T::Annotated { arg, .. } = *self.tpd.tree(tpt) {
            parts.push(tpt);
            tpt = arg;
        }
        (parts.len() > 1).then_some(parts)
    }
}
