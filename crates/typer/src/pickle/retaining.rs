// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/tasty/TreePickler.scala
//   compiler/src/dotty/tools/dotc/core/Annotations.scala
//! Capture-set annotations (`@retains`) in pickled types (dotty's `CompactAnnotation` case of
//! `pickleNewType`).

use super::{Refusal, TreePickler};
use crate::tpd::{TNodeId, Tree};
use crate::typer::capture_types::is_retains_like;
use crate::typer::seen_from::normalized;
use names::nme;
use tasty::tags::{ANNOTATEDtype, APPLY, NEW, SELECTin, TYPEAPPLY};
use tasty::{Kind, SymbolId, Type, TypeId, Universe};

impl TreePickler<'_> {
    /// Type `parent @annot` if `annot` is a capture-set annotation class type, as `parent` and
    /// the type `annot`, `retains[C]` of capture set `C`, or below source version 3.9 the tree
    /// `new retains[C]()` without positions (dotty's `oldTree`); `None` for another annotation.
    /// Refuses a capture set that refers to other than static definitions.
    pub fn retaining(&mut self, parent: TypeId, annot: TypeId) -> Option<Result<(), Refusal>> {
        if !is_retains_like(self.u, annot) {
            return None;
        }
        let own = self.type_tree.and_then(|t| self.capture_classes.get(&t).copied());
        if !is_static_capture_set(self.u, annot, own) {
            return Some(Err(
                "an annotated type of a capture set that refers to other than static definitions",
            ));
        }
        Some(self.with_length(ANNOTATEDtype, &mut |p| {
            p.ty(parent, false)?;
            match p.compact_annotations {
                true => p.ty(annot, false),
                false => p.old_retains(annot),
            }
        }))
    }

    /// `new C[Ts]()` of capture-set annotation type `annot`, `C[Ts]` or `C`, through the class's
    /// primary constructor (dotty's `New(tpe, Nil)`).
    fn old_retains(&mut self, annot: TypeId) -> Result<(), Refusal> {
        let (tycon, args) = match self.u.applied(annot) {
            Some((tycon, args)) => (tycon, args.to_vec()),
            None => (annot, Vec::new()),
        };
        let class = self.u.class_symbol(tycon).ok_or("a capture-set annotation of no class")?;
        let init = self.u.decls_named(class, nme::CONSTRUCTOR).into_iter().next();
        let init = init.ok_or("a capture-set annotation without a constructor")?;
        let select = |p: &mut Self| {
            p.with_length(SELECTin, &mut |p| {
                p.signed_name(init, true)?;
                p.byte(NEW);
                p.ty(tycon, false)?;
                p.ty(tycon, false)
            })
        };
        self.with_length(APPLY, &mut |p| match args.is_empty() {
            true => select(p),
            false => p.with_length(TYPEAPPLY, &mut |p| {
                select(p)?;
                args.iter().try_for_each(|&a| p.ty(a, false))
            }),
        })
    }

    /// Records the info of value or parameter `sym` as the type of its type tree `tpt` if the
    /// tree is synthetic, as a class parameter's field, and has the info's type without its
    /// capture-set annotations: `label: Label[T]^{}` of `class Break[T](val label: Label[T]^{})`.
    /// Refuses such an info with a capture set that refers to other than static definitions and
    /// the `this` of `sym`'s class, as `b: B^{a}` of `class C(val a: A^, val b: B^{a})`.
    pub fn keep_captures(&mut self, sym: SymbolId, tpt: TNodeId) -> Result<(), Refusal> {
        let (Tree::TypeTree { inferred: false }, Some(tpe)) = (self.tpd.tree(tpt), self.tpd.get(tpt).tpe)
        else {
            return Ok(());
        };
        if !matches!(self.u.symbol(sym).kind, Kind::Val | Kind::Param) {
            return Ok(());
        }
        let info = self.u.info(sym);
        if info == tpe || normalized(self.u, info) != tpe {
            return Ok(());
        }
        let mut captures = Vec::new();
        self.u.map_type(info, &mut |u, part| {
            if let Type::Annotated(_, annot) = *u.ty(part)
                && is_retains_like(u, annot)
            {
                captures.push(annot);
            }
            None
        });
        let class =
            std::iter::successors(Some(sym), |&s| self.u.symbol(s).owner).find(|&s| self.u.is_class(s));
        if captures.iter().any(|&c| !is_static_capture_set(self.u, c, class)) {
            return Err(
                "a synthetic type tree of a capture set that refers to other than static definitions",
            );
        }
        if !captures.is_empty() {
            self.kept_captures.insert(tpt, info);
            if let Some(class) = class {
                self.capture_classes.insert(tpt, class);
            }
        }
        Ok(())
    }
}

/// Whether capture-set annotation type `annot` refers to no `this`, parameter or definition other
/// than a static one or the `this` of class `own`, which mapping a type that it annotates leaves
/// alone.
fn is_static_capture_set(u: &mut Universe, annot: TypeId, own: Option<SymbolId>) -> bool {
    let mut is_static = true;
    u.map_type(annot, &mut |u, part| {
        is_static &= match *u.ty(part) {
            Type::This(c) => match *u.ty(c) {
                Type::Package(_) | Type::PackageName(_) => true,
                _ => u.class_symbol(c).is_some_and(|c| u.is_static_owner(c) || Some(c) == own),
            },
            Type::Sym(_, s) => u.is_static_symbol(s),
            Type::Local(..) | Type::Param(..) | Type::RecThis(_) | Type::Skolem(..) | Type::Var(_) => false,
            _ => true,
        };
        None
    });
    is_static
}
