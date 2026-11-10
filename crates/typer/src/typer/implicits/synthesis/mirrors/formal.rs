// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Synthesizer.scala
//! The expected types of the mirrors and the refined types they are cast to (dotty's
//! `checkFormal`, `mirrorCore`, `checkRefinement`).

use super::{MISSING, MirrorSynthesis};
use crate::defn::{MIRROR, MIRROR_PRODUCT, MIRROR_SUM};
use crate::typer::Typer;
use crate::typer::type_ops::TypeOps;
use tasty::{Type, TypeId, Universe};

/// The mirrors an expected type admits.
#[derive(Clone, Copy, PartialEq)]
pub enum MirrorKind {
    Any,
    Product,
    Sum,
}

/// An expected `Mirror` type and the `MirroredType` it refines.
#[derive(Clone, Copy)]
pub struct MirrorFormal {
    pub pt: TypeId,
    pub kind: MirrorKind,
    pub mirrored: TypeId,
}

/// The type members of a mirror's refined type besides its parent.
pub struct MirrorCore<'a> {
    pub parent: &'a str,
    pub mono: TypeId,
    pub label: &'a str,
    /// `MirroredElemTypes` and `MirroredElemLabels`.
    pub elems: Option<(TypeId, TypeId)>,
}

impl MirrorFormal {
    /// Recognizes an expected type such as `Mirror.Of[T]`, `Mirror.ProductOf[T]` or
    /// `Mirror.SumOf[T]`, with any further type members it asks for, and the type `T` it mirrors;
    /// `None` for any other expected type.
    pub fn of(u: &mut Universe, pt: TypeId) -> Option<MirrorFormal> {
        let mut t = u.dealias(pt);
        let mut mirrored = None;
        while let Type::Refined(parent, name, info) = u.ty(t).clone() {
            let info = match *u.ty(info) {
                Type::Method { .. } | Type::Poly { .. } | Type::ByName(_) => return None,
                Type::Alias(a) | Type::Bounds(a, _) => a,
                _ => info,
            };
            if name == "MirroredType" && mirrored.is_none() {
                mirrored = Some(info);
            }
            t = u.dealias(parent);
        }
        let base = u.class_symbol(t).map(|c| u.full_name(c));
        let kind = match base.as_deref()? {
            MIRROR => MirrorKind::Any,
            MIRROR_PRODUCT => MirrorKind::Product,
            MIRROR_SUM => MirrorKind::Sum,
            _ => return None,
        };
        Some(MirrorFormal { pt, kind, mirrored: mirrored? })
    }

    /// Determines the mono type and the element types of a mirror whose elements are `elems`. A
    /// mirror of a type constructor, such as `Mirror.Of[[X] =>> Box[X]]`, gets the constructor
    /// applied to the bounds of its parameters as mono type and its elements as a type lambda
    /// (dotty's `mkMirroredMonoType`).
    pub fn mono_and_elems(&self, u: &mut Universe, elems: TypeId) -> (TypeId, TypeId) {
        let Type::Lambda { binder, params, result } = u.ty(self.mirrored).clone() else {
            return (self.mirrored, elems);
        };
        let (names, bounds): (Vec<String>, Vec<TypeId>) = params.iter().cloned().unzip();
        let mono = u.subst(result, binder, &names, &bounds);
        (mono, u.intern_type(Type::Lambda { binder, params, result: elems }))
    }

    /// Refuses a mirror whose expected type asks for other element types or labels than `types`
    /// and `labels`, e.g. `Mirror.ProductOf[P] { type MirroredElemTypes = (Int, Int) }` for a
    /// `case class P(s: String)`.
    pub fn check_elems(
        &self,
        ops: &mut TypeOps<'_>,
        types: TypeId,
        labels: TypeId,
    ) -> Result<(), &'static str> {
        check_refinement(ops, self.pt, "MirroredElemTypes", types)?;
        check_refinement(ops, self.pt, "MirroredElemLabels", labels)
    }
}

impl MirrorSynthesis {
    /// Builds the type the synthesized mirror is cast to: the mirror class of `core` with its
    /// mirrored type, mono type, label and, for a product or sum, its element types and labels,
    /// combined with the expected type. Refuses an expected type that asks for other elements.
    pub fn mirror_type(self, t: &mut Typer<'_>, core: MirrorCore) -> Result<TypeId, &'static str> {
        let formal = self.formal;
        let parent = t.resolve_full_name(core.parent, true).ok_or(MISSING)?;
        let label = t.u.intern_type(Type::Str(core.label.to_owned()));
        let mut members = vec![
            ("MirroredMonoType", core.mono),
            ("MirroredType", formal.mirrored),
            ("MirroredLabel", label),
        ];
        if let Some((types, labels)) = core.elems {
            formal.check_elems(&mut t.types(), types, labels)?;
            members.extend([("MirroredElemTypes", types), ("MirroredElemLabels", labels)]);
        }
        let mut tpe = crate::defn::type_ref(t.u, parent);
        for (name, member) in members {
            let alias = t.u.intern_type(Type::Alias(member));
            tpe = t.u.intern_type(Type::Refined(tpe, name.to_owned(), alias));
        }
        Ok(match t.u.is_subtype(tpe, formal.pt) {
            true => tpe,
            false => {
                let pt = t.u.dealias(formal.pt);
                t.u.intern_type(Type::And(pt, tpe))
            }
        })
    }
}

/// Refuses a mirror whose expected type `pt` asks for a type member `name` that `expected` does
/// not fit (dotty reports it).
fn check_refinement(
    ops: &mut TypeOps<'_>,
    pt: TypeId,
    name: &str,
    expected: TypeId,
) -> Result<(), &'static str> {
    let Some(info) = ops.u.refinement_named(pt, name) else { return Ok(()) };
    let (lo, hi) = match *ops.u.ty(info) {
        Type::Alias(a) => (a, a),
        Type::Bounds(lo, hi) => (lo, hi),
        _ => (info, info),
    };
    match ops.conforms(lo, expected) && ops.conforms(expected, hi) {
        true => Ok(()),
        false => Err("Mirror whose element types or labels mismatch the expected ones"),
    }
}
