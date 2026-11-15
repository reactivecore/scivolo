// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/tasty/TreePickler.scala
//! Annotated and refined types and type trees (dotty's `AnnotatedType` case of
//! `pickleNewType`, `Annotated` and `RefinedTypeTree` cases of `pickleTree`).

use super::annotation_origins::AnnotationOrigins;
use super::{Refusal, TreePickler};
use crate::defn::type_ref;
use crate::tpd::{List, TNodeId, Tree};
use names::nme;
use tasty::tags::{ANNOTATEDtpt, ANNOTATEDtype, APPLY, NEW, REFINEDtpt, SELECTin};
use tasty::{Type, TypeId};

impl TreePickler<'_> {
    /// Type `annotated`, `parent` annotated with class type `annot`: with the annotation tree an
    /// annotated expression of the file gave it, else `new annot()`; told from the others by its
    /// tree when keyed. Refuses a type several annotated expressions gave, outside their type trees
    /// and the type trees derived from or inferred from one of them (`AnnotationOrigins`), a type
    /// of a derived type tree with several annotated type trees of `annot` it may come from, and a
    /// type traced back to one of several annotation trees of the file where that one has
    /// arguments.
    pub fn annotated(&mut self, annotated: TypeId, parent: TypeId, annot: TypeId) -> Result<(), Refusal> {
        let several = "an annotated type of several annotation trees";
        let tree = match self.tpt_annotation(annotated) {
            Some(tree) => Some(tree.ok_or(several)?),
            None => match self
                .selector_annotation(annotated)
                .or_else(|| self.origin_annotation(annotated, annot))
            {
                Some(Some(tree))
                    if self.tpd.type_annotation(annotated) == Some(None)
                        && self.has_arguments(tree, annot) =>
                {
                    return Err("an annotated type traced to one of several annotation trees with arguments");
                }
                Some(origin) => Some(origin.ok_or(several)?),
                None => self.tpd.type_annotation(annotated).map(|t| t.ok_or(several)).transpose()?,
            },
        };
        if let Some(tree) = tree {
            if !self.keying {
                self.type_annots.push(tree);
            }
            return self.with_length(ANNOTATEDtype, &mut |p| {
                p.ty(parent, false)?;
                if !p.keying {
                    return p.tree(tree);
                }
                let key = p.tree_key(tree);
                p.buf.buf.write_bytes(&key.to_le_bytes());
                Ok(())
            });
        }
        self.with_length(ANNOTATEDtype, &mut |p| {
            p.ty(parent, false)?;
            p.new_annotation(annot)
        })
    }

    /// The annotation tree of the type tree pickled, else of the type trees of the innermost
    /// enclosing pattern, case pattern or super accessor's member that have one, whose type is
    /// `annotated` and an annotated expression gave, or that annotate `annotated`'s parent with its
    /// annotation class (a capture type tree, typed without its annotation); `Some(None)` if those
    /// type trees give several annotation trees, one with arguments.
    pub fn tpt_annotation(&self, annotated: TypeId) -> Option<Option<TNodeId>> {
        let parts = match *self.u.ty(annotated) {
            Type::Annotated(parent, class) => Some((parent, class)),
            _ => None,
        };
        let gives = |tpt: TNodeId| {
            let annotates = match (self.tpd.tree(tpt), parts) {
                (&Tree::Annotated { arg, annot }, Some((parent, class))) => {
                    self.tpd.get(arg).tpe == Some(parent) && self.tpd.get(annot).tpe == Some(class)
                }
                _ => false,
            };
            (self.tpd.tpe(tpt) == annotated || annotates).then(|| self.annotation_of(tpt)).flatten()
        };
        if let Some(tree) = self.type_tree.and_then(gives) {
            return Some(Some(tree));
        }
        let class = parts.map_or(annotated, |(_, c)| c);
        self.typed_tpts.iter().rev().find_map(|layer| {
            let trees: Vec<TNodeId> = layer.iter().rev().filter_map(|&t| gives(t)).collect();
            let first = *trees.first()?;
            let several =
                trees.iter().any(|&t| t != first) && trees.iter().any(|&t| self.has_arguments(t, class));
            Some((!several).then_some(first))
        })
    }

    /// The annotation tree of `annotated`, of annotation class `annot` and part of the type of the
    /// type tree pickled, that the type tree it derives from or the right-hand side it is inferred
    /// from gave, `Some(None)` if several may have; marks the type keyed as told by its place.
    fn origin_annotation(&mut self, annotated: TypeId, annot: TypeId) -> Option<Option<TNodeId>> {
        let tpt = self.type_tree?;
        let origins = self.origins.get_or_insert_with(|| AnnotationOrigins::new(self.u, self.tpd, self.root));
        let tree = origins.of_type_tree(self.u, self.tpd, tpt, annotated, annot)?;
        self.keyed_by_place = true;
        Some(tree)
    }

    /// The annotation tree of `annotated` in a pattern that the match's selector gives (dotty's
    /// patterns share the selector's type), `Some(None)` if several may.
    fn selector_annotation(&mut self, annotated: TypeId) -> Option<Option<TNodeId>> {
        let selector = self.pattern_selector?;
        let origins = self.origins.get_or_insert_with(|| AnnotationOrigins::new(self.u, self.tpd, self.root));
        let tree = origins.of_expr(self.u, self.tpd, selector, annotated)?;
        self.keyed_by_place = true;
        Some(tree)
    }

    /// Whether annotation tree `tree` of class type `annot` is other than `new C()`: it has value
    /// or type arguments.
    fn has_arguments(&self, tree: TNodeId, annot: TypeId) -> bool {
        let Tree::Apply { fun, args } = *self.tpd.tree(tree) else { return true };
        let simple_new = matches!(self.tpd.tree(fun), Tree::Select { .. });
        !self.tpd.items(args).is_empty() || !simple_new || matches!(self.u.ty(annot), Type::Applied(..))
    }

    /// The annotation tree of type tree `tpt`, if an annotated expression or type gave its type.
    fn annotation_of(&self, tpt: TNodeId) -> Option<TNodeId> {
        match *self.tpd.tree(tpt) {
            Tree::Annotated { annot, .. } => Some(annot),
            _ => self.tpd.tpt_annotation(tpt),
        }
    }

    /// Whether class type `class` is capture checking's `caps.internal.inferred`.
    pub fn is_inferred_mark(&mut self, class: TypeId) -> bool {
        self.u.is_class_named(class, crate::defn::INFERRED)
    }

    /// `new C()` of class type `class`, an annotation without arguments, through its constructor
    /// of no parameters. Refuses a generic class, e.g. capture checking's `retains[T]`, whose
    /// type arguments the annotated type does not keep.
    pub fn new_annotation(&mut self, class: TypeId) -> Result<(), Refusal> {
        let c = self.u.class_symbol(class).ok_or("an annotation of no class")?;
        if !self.u.type_params(c).is_empty() {
            return Err("an annotated type of a generic annotation class without its annotation tree");
        }
        let ctors = self.u.decls_named(c, nme::CONSTRUCTOR);
        let init = ctors.into_iter().find(|&c| self.u.erased_method(c).0.is_empty());
        let init = init.ok_or("a type annotation without a constructor of no parameters")?;
        let ids = (u32::MAX - 2 * self.annots.len() as u32, u32::MAX - 2 * self.annots.len() as u32 - 1);
        if let Some(tpt) = self.type_tree.filter(|_| !self.keying) {
            self.annots.push((ids.0, ids.1, tpt));
            self.buf.register_tree(ids.0);
        }
        self.with_length(APPLY, &mut |p| {
            p.with_length(SELECTin, &mut |p| {
                p.signed_name(init, true)?;
                p.byte(NEW);
                if p.type_tree.is_some() && !p.keying {
                    p.buf.register_tree(ids.1);
                }
                p.ty(class, false)?;
                p.ty(class, false)
            })
        })
    }

    /// Type tree `tpt { refinements }`, the refinements members of a synthetic class whose
    /// references share the tree (dotty's `RefinedTypeTree` case of `pickleTree`).
    pub fn refined_tree(&mut self, tpt: TNodeId, refinements: List) -> Result<(), Refusal> {
        let refinements = self.tpd.items(refinements).to_vec();
        let Some(&first) = refinements.first() else { return self.tree(tpt) };
        let member = self.tpd.sym(first).ok_or("a refinement without a symbol")?;
        let class = self.u.symbol(member).owner.ok_or("a refinement without a class")?;
        self.register_def(class);
        let class_ref = type_ref(self.u, class);
        self.share_here(class_ref)?;
        self.pre_register(&refinements);
        self.with_length(REFINEDtpt, &mut |p| {
            p.tree(tpt)?;
            refinements.iter().try_for_each(|&r| p.tree(r))
        })
    }

    /// Type tree `arg` annotated with constructor call `annot`.
    pub fn annotated_tree(&mut self, arg: TNodeId, annot: TNodeId) -> Result<(), Refusal> {
        self.with_length(ANNOTATEDtpt, &mut |p| {
            p.tree(arg)?;
            p.tree(annot)
        })
    }
}
