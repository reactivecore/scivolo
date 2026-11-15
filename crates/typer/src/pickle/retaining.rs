// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/tasty/TreePickler.scala
//! Capture-set annotations (`@retains`) in pickled types (dotty's `CompactAnnotation` case of
//! `pickleNewType`).

use super::{Refusal, TreePickler};
use crate::tpd::{TNodeId, Tree};
use crate::typer::seen_from::{is_retains_like, normalized};
use tasty::tags::ANNOTATEDtype;
use tasty::{Kind, SymbolId, Type, TypeId, Universe};

impl TreePickler<'_> {
    /// Type `parent @annot` if `annot` is a capture-set annotation class type whose capture set
    /// refers to static definitions only, as `parent` and the type `annot`, `retains[C]` of
    /// capture set `C`; `None` for another annotation or capture set, and under a source version
    /// below 3.9, whose TASTy has the annotation's tree.
    pub fn retaining(&mut self, parent: TypeId, annot: TypeId) -> Option<Result<(), Refusal>> {
        let compact = self.file.is_none_or(|f| f.attributes.compact_annotations);
        if !compact || !is_retains_like(self.u, annot) || !is_static_capture_set(self.u, annot) {
            return None;
        }
        Some(self.with_length(ANNOTATEDtype, &mut |p| {
            p.ty(parent, false)?;
            p.ty(annot, false)
        }))
    }

    /// Records the info of value or parameter `sym` as the type of its type tree `tpt` if the
    /// tree is synthetic, as a class parameter's field, and has the info's type without its
    /// capture-set annotations: `label: Label[T]^{}` of `class Break[T](val label: Label[T]^{})`.
    /// Refuses such an info with a capture set that refers to other than static definitions, as
    /// `b: B^{a}` of `class C(val a: A^, val b: B^{a})`. Leaves the type tree alone under a source
    /// version below 3.9.
    pub fn keep_captures(&mut self, sym: SymbolId, tpt: TNodeId) -> Result<(), Refusal> {
        let (Tree::TypeTree { inferred: false }, Some(tpe)) = (self.tpd.tree(tpt), self.tpd.get(tpt).tpe)
        else {
            return Ok(());
        };
        let compact = self.file.is_none_or(|f| f.attributes.compact_annotations);
        if !compact || !matches!(self.u.symbol(sym).kind, Kind::Val | Kind::Param) {
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
        if captures.iter().any(|&c| !is_static_capture_set(self.u, c)) {
            return Err(
                "a synthetic type tree of a capture set that refers to other than static definitions",
            );
        }
        if !captures.is_empty() {
            self.kept_captures.insert(tpt, info);
        }
        Ok(())
    }
}

/// Whether capture-set annotation type `annot` refers to no `this`, parameter or definition other
/// than a static one, which mapping a type that it annotates leaves alone.
fn is_static_capture_set(u: &mut Universe, annot: TypeId) -> bool {
    let mut is_static = true;
    u.map_type(annot, &mut |u, part| {
        is_static &= match *u.ty(part) {
            Type::This(c) => match *u.ty(c) {
                Type::Package(_) | Type::PackageName(_) => true,
                _ => u.class_symbol(c).is_some_and(|c| u.is_static_owner(c)),
            },
            Type::Sym(_, s) => u.is_static_symbol(s),
            Type::Local(..) | Type::Param(..) | Type::RecThis(_) | Type::Skolem(..) | Type::Var(_) => false,
            _ => true,
        };
        None
    });
    is_static
}
