// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/tasty/TreePickler.scala
//! Annotated and refined types and type trees (dotty's `AnnotatedType` case of
//! `pickleNewType`, `Annotated` and `RefinedTypeTree` cases of `pickleTree`).

use super::{Refusal, TreePickler};
use crate::defn::type_ref;
use crate::tpd::{List, TNodeId, Tree};
use names::nme;
use tasty::TypeId;
use tasty::tags::{ANNOTATEDtpt, ANNOTATEDtype, APPLY, NEW, REFINEDtpt, SELECTin};

impl TreePickler<'_> {
    /// Type `annotated`, `parent` annotated with class type `annot`: with the annotation tree an
    /// annotated expression of the file gave it, else `new annot()`; told from the others by its
    /// tree when keyed. Refuses a type several annotated expressions gave, outside their type trees.
    pub fn annotated(&mut self, annotated: TypeId, parent: TypeId, annot: TypeId) -> Result<(), Refusal> {
        let tree = match self.tpt_annotation(annotated) {
            Some(tree) => Some(tree),
            None => match self.tpd.type_annotation(annotated) {
                Some(None) => return Err("an annotated type of several annotation trees"),
                given => given.flatten(),
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

    /// The annotation tree of the type tree pickled, else of the innermost enclosing typed
    /// expression or case pattern, whose type is `annotated` and an annotated expression gave.
    pub fn tpt_annotation(&self, annotated: TypeId) -> Option<TNodeId> {
        let enclosing = self.typed_tpts.iter().rev().copied();
        let tpts = self.type_tree.into_iter().chain(enclosing);
        tpts.filter(|&tpt| self.tpd.tpe(tpt) == annotated).find_map(|tpt| self.annotation_of(tpt))
    }

    /// The annotation tree of type tree `tpt`, if an annotated expression or type gave its type.
    fn annotation_of(&self, tpt: TNodeId) -> Option<TNodeId> {
        match *self.tpd.tree(tpt) {
            Tree::Annotated { annot, .. } => Some(annot),
            _ => self.tpd.tpt_annotation(tpt),
        }
    }

    /// The type trees of the typed expressions and patterns in tree `t`, and the annotated type
    /// trees in them.
    pub fn annotated_tpts(&self, t: TNodeId) -> Vec<TNodeId> {
        let mut tpts = match *self.tpd.tree(t) {
            Tree::Typed { tpt, .. } => vec![tpt],
            Tree::Annotated { .. } => vec![t],
            _ => Vec::new(),
        };
        tpts.extend(self.tpd.children(t).into_iter().flat_map(|c| self.annotated_tpts(c)));
        tpts
    }

    /// The type trees of `annotated_tpts(rhs)`, if a definition's type tree `tpt` is inferred,
    /// with an annotation tree that no other of them gives another one for the same type.
    pub fn rhs_tpts(&self, tpt: TNodeId, rhs: Option<TNodeId>) -> Vec<TNodeId> {
        let Some(rhs) = rhs.filter(|_| matches!(self.tpd.tree(tpt), Tree::TypeTree { .. })) else {
            return Vec::new();
        };
        let tpts: Vec<_> =
            self.annotated_tpts(rhs).into_iter().filter(|&t| self.annotation_of(t).is_some()).collect();
        let unique = |&t: &TNodeId| {
            let same = |o: &&TNodeId| self.tpd.tpe(**o) == self.tpd.tpe(t);
            tpts.iter().filter(same).all(|&o| self.annotation_of(o) == self.annotation_of(t))
        };
        tpts.iter().copied().filter(unique).collect()
    }

    /// Pickles `pickle` inside typed expressions or case patterns of type trees `tpts`.
    pub fn with_typed_tpts(
        &mut self,
        tpts: Vec<TNodeId>,
        pickle: &mut dyn FnMut(&mut Self) -> Result<(), Refusal>,
    ) -> Result<(), Refusal> {
        let depth = self.typed_tpts.len();
        self.typed_tpts.extend(tpts);
        let pickled = pickle(self);
        self.typed_tpts.truncate(depth);
        pickled
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
