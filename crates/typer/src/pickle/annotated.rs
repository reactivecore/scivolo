// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/tasty/TreePickler.scala
//! Annotated and refined types and type trees (dotty's `AnnotatedType` case of
//! `pickleNewType`, `Annotated` and `RefinedTypeTree` cases of `pickleTree`).

use super::{Refusal, TreePickler};
use crate::defn::type_ref;
use crate::tpd::{List, TNodeId};
use tasty::tags::{ANNOTATEDtpt, ANNOTATEDtype, APPLY, NEW, REFINEDtpt, SELECTin};
use tasty::{TypeId, nme};

impl TreePickler<'_> {
    /// Type `parent` annotated with class type `annot`, the annotation `new annot()`.
    pub fn annotated(&mut self, parent: TypeId, annot: TypeId) -> Result<(), Refusal> {
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
