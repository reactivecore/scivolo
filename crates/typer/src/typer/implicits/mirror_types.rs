// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Synthesizer.scala
//! The component that makes one `Mirror` argument, the expected types of the mirrors it makes and
//! the refined types it casts them to (dotty's `makeMirror`, `checkFormal`, `mirrorCore`,
//! `checkRefinement`).

use super::super::Typer;
use super::super::type_ops::TypeOps;
use crate::ctx::Ctx;
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::Span;
use tasty::{Type, TypeId, Universe};

/// The refusal when the library lacks a class a mirror needs.
pub const MISSING: &str = "Mirror without its library classes";

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

/// Makes the `Mirror` argument `formal` asks for (dotty's `synthesizedMirror` and what it calls);
/// its trees are placed at `at` and typed in `ctx`.
#[derive(Clone, Copy)]
pub struct MirrorSynthesis {
    pub formal: MirrorFormal,
    pub at: Span,
    pub ctx: Ctx,
}

impl MirrorFormal {
    /// Reads `pt` as `Mirror`, `Mirror.Product` or `Mirror.Sum` refined by type members, among
    /// them `MirroredType`; `None` for another type.
    pub fn of(u: &mut Universe, pt: TypeId) -> Option<MirrorFormal> {
        let mut t = u.dealias(pt);
        let mut mirrored = None;
        while let Type::Refined(parent, name, info) = u.types().get(t).clone() {
            let info = match *u.types().get(info) {
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
            "scala.deriving.Mirror" => MirrorKind::Any,
            "scala.deriving.Mirror$.Product" => MirrorKind::Product,
            "scala.deriving.Mirror$.Sum" => MirrorKind::Sum,
            _ => return None,
        };
        Some(MirrorFormal { pt, kind, mirrored: mirrored? })
    }

    /// Determines the `MirroredMonoType` and `MirroredElemTypes` of the mirrored type with element
    /// types `elems`: of a type lambda, its result with each parameter replaced by its bounds, and
    /// the lambda of the elements (dotty's `mkMirroredMonoType`).
    pub fn mono_and_elems(&self, u: &mut Universe, elems: TypeId) -> (TypeId, TypeId) {
        let Type::Lambda { binder, params, result } = u.types().get(self.mirrored).clone() else {
            return (self.mirrored, elems);
        };
        let (names, bounds): (Vec<String>, Vec<TypeId>) = params.iter().cloned().unzip();
        let mono = u.subst(result, binder, &names, &bounds);
        (mono, u.intern_type(Type::Lambda { binder, params, result: elems }))
    }

    /// Refuses refinements of `MirroredElemTypes` and `MirroredElemLabels` in the expected type
    /// that do not admit `types` and `labels`.
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
    /// Builds `parent { MirroredMonoType = mono; MirroredType = mirrored; MirroredLabel = label;
    /// MirroredElemTypes = …; MirroredElemLabels = … }` intersected with the expected type;
    /// refused if the expected type refines the element types or labels otherwise.
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

    /// Creates `new C(args)` of the class of full name `full`.
    pub fn new_with_args(
        self,
        t: &mut Typer<'_>,
        full: &str,
        args: &[TNodeId],
    ) -> Result<TNodeId, &'static str> {
        let at = self.at;
        let class = t.resolve_full_name(full, true).ok_or(MISSING)?;
        let ctor = t.u.decl(class, "<init>").ok().and_then(|d| d.first().copied()).ok_or(MISSING)?;
        let raw = crate::defn::type_ref(t.u, class);
        let tpt = t.tpd.node(T::TypeTree { inferred: false }, at, Some(raw), None);
        let new = t.tpd.node(T::New { tpt }, at, Some(raw), None);
        let select = t.build().select(new, ctor, at);
        Ok(t.build().apply(select, args, raw, at))
    }

    /// Casts `mirror` to the refined mirror type `to`.
    pub fn cast(
        self,
        t: &mut Typer<'_>,
        mirror: TNodeId,
        to: TypeId,
    ) -> Result<Option<TNodeId>, &'static str> {
        t.build().cast_to(mirror, to, self.at).map(Some)
    }
}

/// Makes the labels `labels` a tuple of string constant types.
pub fn label_pairs(t: &mut Typer<'_>, labels: &[String]) -> Result<TypeId, &'static str> {
    let labels: Vec<TypeId> = labels.iter().map(|l| t.u.intern_type(Type::Str(l.clone()))).collect();
    t.nested_pairs(&labels)
}

/// Refuses a refinement of `name` in `pt` that does not admit `expected` (dotty reports it).
fn check_refinement(
    ops: &mut TypeOps<'_>,
    pt: TypeId,
    name: &str,
    expected: TypeId,
) -> Result<(), &'static str> {
    let Some(info) = ops.u.refinement_named(pt, name) else { return Ok(()) };
    let (lo, hi) = match *ops.u.types().get(info) {
        Type::Alias(a) => (a, a),
        Type::Bounds(lo, hi) => (lo, hi),
        _ => (info, info),
    };
    match ops.conforms(lo, expected) && ops.conforms(expected, hi) {
        true => Ok(()),
        false => Err("Mirror whose element types or labels mismatch the expected ones"),
    }
}
