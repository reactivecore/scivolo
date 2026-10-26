// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Synthesizer.scala
//! The expected types of the mirrors the compiler makes and the refined types it casts them to
//! (dotty's `makeMirror`, `checkFormal`, `mirrorCore`, `checkRefinement`).

use super::super::Typer;
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::{Span, TName};
use tasty::{Type, TypeId};

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

impl Typer<'_> {
    /// `pt` as `Mirror`, `Mirror.Product` or `Mirror.Sum` refined by type members, among them
    /// `MirroredType`; `None` for another type.
    pub fn mirror_formal(&mut self, pt: TypeId) -> Option<MirrorFormal> {
        let mut t = self.u.dealias(pt);
        let mut mirrored = None;
        while let Type::Refined(parent, name, info) = self.u.types().get(t).clone() {
            let info = match self.u.types().get(info).clone() {
                Type::Method { .. } | Type::Poly { .. } | Type::ByName(_) => return None,
                Type::Alias(a) | Type::Bounds(a, _) => a,
                _ => info,
            };
            if name == "MirroredType" && mirrored.is_none() {
                mirrored = Some(info);
            }
            t = self.u.dealias(parent);
        }
        let base = self.u.class_symbol(t).map(|c| self.u.full_name(c));
        let kind = match base.as_deref()? {
            "scala.deriving.Mirror" => MirrorKind::Any,
            "scala.deriving.Mirror$.Product" => MirrorKind::Product,
            "scala.deriving.Mirror$.Sum" => MirrorKind::Sum,
            _ => return None,
        };
        Some(MirrorFormal { pt, kind, mirrored: mirrored? })
    }

    /// `parent { MirroredMonoType = mono; MirroredType = mirrored; MirroredLabel = label;
    /// MirroredElemTypes = …; MirroredElemLabels = … }` intersected with the expected type;
    /// refused if the expected type refines the element types or labels otherwise.
    pub fn mirror_type(&mut self, formal: &MirrorFormal, core: MirrorCore) -> Result<TypeId, &'static str> {
        let parent = self.resolve_full_name(core.parent, true).ok_or(MISSING)?;
        let label = self.u.intern_type(Type::Str(core.label.to_owned()));
        let mut members = vec![
            ("MirroredMonoType", core.mono),
            ("MirroredType", formal.mirrored),
            ("MirroredLabel", label),
        ];
        if let Some((types, labels)) = core.elems {
            self.check_refinement(formal.pt, "MirroredElemTypes", types)?;
            self.check_refinement(formal.pt, "MirroredElemLabels", labels)?;
            members.extend([("MirroredElemTypes", types), ("MirroredElemLabels", labels)]);
        }
        let mut t = crate::defn::type_ref(self.u, parent);
        for (name, member) in members {
            let alias = self.u.intern_type(Type::Alias(member));
            t = self.u.intern_type(Type::Refined(t, name.to_owned(), alias));
        }
        Ok(match self.u.is_subtype(t, formal.pt) {
            true => t,
            false => {
                let pt = self.u.dealias(formal.pt);
                self.u.intern_type(Type::And(pt, t))
            }
        })
    }

    /// Refuses a refinement of `name` in `pt` that does not admit `expected` (dotty reports it).
    pub fn check_refinement(&mut self, pt: TypeId, name: &str, expected: TypeId) -> Result<(), &'static str> {
        let Some(info) = self.u.refinement_named(pt, name) else { return Ok(()) };
        let (lo, hi) = match self.u.types().get(info).clone() {
            Type::Alias(a) => (a, a),
            Type::Bounds(lo, hi) => (lo, hi),
            _ => (info, info),
        };
        match self.conforms(lo, expected) && self.conforms(expected, hi) {
            true => Ok(()),
            false => Err("Mirror whose element types or labels mismatch the expected ones"),
        }
    }

    /// `t1 *: t2 *: … *: EmptyTuple` (dotty's `TypeOps.nestedPairs`).
    pub fn nested_pairs(&mut self, elems: &[TypeId]) -> Result<TypeId, &'static str> {
        let pair = self.resolve_full_name("scala.*:", true).ok_or(MISSING)?;
        let empty = self.resolve_full_name("scala.EmptyTuple", false).ok_or(MISSING)?;
        let pair = crate::defn::type_ref(self.u, pair);
        let mut t = self.u.intern_type(Type::Sym(None, empty));
        for &e in elems.iter().rev() {
            t = self.u.intern_type(Type::Applied(pair, vec![e, t]));
        }
        Ok(t)
    }

    /// The `MirroredMonoType` and `MirroredElemTypes` of `mirrored` with element types `elems`:
    /// of a type lambda, its result with each parameter replaced by its bounds, and the lambda
    /// of the elements (dotty's `mkMirroredMonoType`).
    pub fn mono_and_elems(&mut self, mirrored: TypeId, elems: TypeId) -> (TypeId, TypeId) {
        let Type::Lambda { binder, params, result } = self.u.types().get(mirrored).clone() else {
            return (mirrored, elems);
        };
        let (names, bounds): (Vec<String>, Vec<TypeId>) = params.iter().cloned().unzip();
        let mono = self.u.subst(result, binder, &names, &bounds);
        (mono, self.u.intern_type(Type::Lambda { binder, params, result: elems }))
    }

    /// The labels `elems` as a tuple of string constant types.
    pub fn label_pairs(&mut self, labels: &[String]) -> Result<TypeId, &'static str> {
        let labels: Vec<TypeId> = labels.iter().map(|l| self.u.intern_type(Type::Str(l.clone()))).collect();
        self.nested_pairs(&labels)
    }

    /// `new C(args)` of the class of full name `full` at `at`.
    pub fn new_with_args(&mut self, full: &str, args: &[TNodeId], at: Span) -> Result<TNodeId, &'static str> {
        let class = self.resolve_full_name(full, true).ok_or(MISSING)?;
        let ctor = self.u.decl(class, "<init>").ok().and_then(|d| d.first().copied()).ok_or(MISSING)?;
        let raw = crate::defn::type_ref(self.u, class);
        let tpt = self.tpd.node(T::TypeTree { inferred: false }, at, Some(raw), None);
        let new = self.tpd.node(T::New { tpt }, at, Some(raw), None);
        let select = self.build().select(new, ctor, at);
        Ok(self.build().apply(select, args, raw, at))
    }

    /// `tree.$asInstanceOf$[to]` at `at`.
    pub fn cast_to(&mut self, tree: TNodeId, to: TypeId, at: Span) -> Result<TNodeId, &'static str> {
        let any = self.u.builtins().any;
        let m = self.u.decl(any, "$asInstanceOf$").ok().and_then(|d| d.first().copied()).ok_or(MISSING)?;
        let select_tpe = self.u.intern_type(Type::Sym(Some(self.tpd.tpe(tree)), m));
        let name = TName { name: self.names.intern("$asInstanceOf$"), is_type: false };
        let fun = self.tpd.node(T::Select { qualifier: tree, name }, at, Some(select_tpe), Some(m));
        let tpt = self.tpd.node(T::TypeTree { inferred: false }, at, Some(to), None);
        let args = self.tpd.list(&[tpt]);
        Ok(self.tpd.node(T::TypeApply { fun, args }, at, Some(to), None))
    }
}
