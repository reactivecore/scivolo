// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/tasty/TreePickler.scala
//! Annotated and refined types and type trees (dotty's `AnnotatedType` case of
//! `pickleNewType`, `Annotated` and `RefinedTypeTree` cases of `pickleTree`), and the order of a
//! definition's annotations.

use super::{Refusal, TreePickler};
use crate::defn::type_ref;
use crate::tpd::{List, TNodeId, Tree as T};
use names::nme;
use tasty::defn::{NON_BEAN_TARGETS, UNUSED};
use tasty::tags::{ANNOTATEDtpt, ANNOTATEDtype, APPLY, NEW, REFINEDtpt, SELECTin};
use tasty::{Kind, SymbolId, TypeId};

/// The meta-annotation target of parameters, `scala.annotation.meta.param`.
const PARAM_TARGET: &str = "param";

impl TreePickler<'_> {
    /// Type `annotated`, `parent` annotated with class type `annot`: with the annotation tree an
    /// annotated expression of the file gave it, else `new annot()`.
    pub fn annotated(&mut self, annotated: TypeId, parent: TypeId, annot: TypeId) -> Result<(), Refusal> {
        if let Some(tree) = self.tpd.type_annotation(annotated) {
            if !self.keying {
                self.type_annots.push(tree);
            }
            return self.with_length(ANNOTATEDtype, &mut |p| {
                p.ty(parent, false)?;
                p.tree(tree)
            });
        }
        let class = self.u.class_symbol(annot).ok_or("an annotation of no class")?;
        let ctors = self.u.decls_named(class, nme::CONSTRUCTOR);
        let ctor = ctors.into_iter().find(|&c| self.u.erased_method(c).0.is_empty());
        let ctor = ctor.ok_or("a type annotation without a constructor of no parameters")?;
        self.with_length(ANNOTATEDtype, &mut |p| {
            p.ty(parent, false)?;
            p.with_length(APPLY, &mut |p| {
                p.with_length(SELECTin, &mut |p| {
                    p.signed_name(ctor, true)?;
                    p.byte(NEW);
                    p.ty(annot, false)?;
                    p.ty(annot, false)
                })
            })
        })
    }

    /// The annotation trees of `s`, in order; in a file, the ones of its definition last first
    /// (dotty's `addAnnotation` prepends), and on a value parameter its `@unused` first
    /// (`PostTyper`'s `transform` of a parameter), and of a parameter only those meant for
    /// parameters (`keepAnnotationsCarrying`).
    pub fn annotation_trees(&mut self, s: SymbolId) -> Vec<TNodeId> {
        let own = self.tpd.annotations(s).to_vec();
        let Some(file) = self.file else { return own };
        let mut trees = file.annotations.get(&s).cloned().unwrap_or_else(|| own.clone());
        let at: Vec<usize> = (0..trees.len()).filter(|&i| own.contains(&trees[i])).collect();
        for (&i, &a) in at.iter().zip(own.iter().rev()) {
            trees[i] = a;
        }

        if self.u.symbol(s).kind != Kind::Param {
            return trees;
        }
        let kept = self.u.annotations_carrying(s, &[PARAM_TARGET], NON_BEAN_TARGETS);
        trees.retain(|&a| {
            let t = self.tpd.tpe(a);
            self.u.is_class_named(t, UNUSED) || self.u.class_symbol(t).is_some_and(|c| kept.contains(&c))
        });
        if let Some(i) = trees.iter().position(|&a| self.u.is_class_named(self.tpd.tpe(a), UNUSED)) {
            trees[..=i].rotate_right(1);
        }
        trees
    }

    /// Marks the calls and selections of annotation `annot` of parameter `sym` to be pickled
    /// anew if pickled before, their other parts shared (a field's annotation its constructor
    /// parameter has too).
    pub fn param_annotation_anew(&mut self, sym: SymbolId, annot: TNodeId) {
        if self.u.symbol(sym).kind != Kind::Param || self.buf.addr_of_tree(self.tree_key(annot)).is_none() {
            return;
        }
        let mut t = annot;
        self.anew.insert(t);
        while let T::Apply { fun, .. } | T::TypeApply { fun, .. } = *self.tpd.tree(t) {
            t = fun;
            self.anew.insert(t);
        }
    }

    /// Whether class type `class` is capture checking's `caps.internal.inferred`.
    pub fn is_inferred_mark(&mut self, class: TypeId) -> bool {
        self.u.is_class_named(class, crate::defn::INFERRED)
    }

    /// `new C()` of class type `class`, an annotation without arguments.
    pub fn new_annotation(&mut self, class: TypeId) -> Result<(), Refusal> {
        let c = self.u.class_symbol(class).ok_or("an annotation of no class")?;
        let init = self.u.decl(c, nme::CONSTRUCTOR).ok().and_then(|d| d.first().copied());
        let init = init.ok_or("an annotation class without a constructor")?;
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
