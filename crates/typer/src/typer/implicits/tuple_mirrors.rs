//! `Mirror`s of tuple types (dotty's `productMirror` of a `TupleN` or a generic tuple):
//! `new scala.runtime.TupleMirror(n)` cast to the mirror type.

use super::super::Typer;
use super::super::small_tuples::MAX_TUPLE_ARITY;
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::constant::Constant;
use syntax::ast::{Span, TName};
use tasty::{Type, TypeId};

/// The refinements of a `Mirror` an expected type may have.
const KNOWN: [&str; 3] = ["MirroredType", "MirroredMonoType", "MirroredElemTypes"];
/// The refusal when the library lacks a class a tuple mirror needs.
const MISSING: &str = "Mirror of a tuple without its library classes";

/// What an expected `Mirror` type says: its `MirroredType`, its `MirroredElemTypes` refinement,
/// and whether it has other refinements.
struct MirrorPt {
    mirrored: Option<TypeId>,
    elem_types: Option<TypeId>,
    others: bool,
}

impl Typer<'_> {
    /// The mirror of a tuple type for `pt`, a `Mirror` or `Mirror.Product` refined by its
    /// `MirroredType`; `None` if `pt` is none or mirrors no tuple.
    pub fn synthesized_tuple_mirror(
        &mut self,
        pt: TypeId,
        at: Span,
    ) -> Result<Option<TNodeId>, &'static str> {
        let MirrorPt { mirrored: Some(mirrored), elem_types: refined_elems, others } = self.mirror_pt(pt)?
        else {
            return Ok(None);
        };
        let mirrored = self.zonk(mirrored);
        if !self.is_fully_defined(mirrored) {
            return Err("Mirror of a type with type variables");
        }
        let dealiased = self.u.dealias(mirrored);
        let elems = match self.u.types().get(dealiased).clone() {
            Type::Applied(f, args)
                if self
                    .u
                    .class_symbol(f)
                    .is_some_and(|c| self.u.full_name(c) == format!("scala.Tuple{}", args.len())) =>
            {
                args
            }
            _ => match self.tuple_elements(dealiased) {
                Some(elems) if !elems.is_empty() => elems,
                _ => return Ok(None),
            },
        };
        if others {
            return Err("Mirror of a tuple with refinements other than its mirrored types");
        }
        let product = self.resolve_full_name("scala.deriving.Mirror$.Product", true).ok_or(MISSING)?;
        let runtime = self.resolve_full_name("scala.runtime.TupleMirror", true).ok_or(MISSING)?;
        let labels: Vec<TypeId> =
            (1..=elems.len()).map(|i| self.u.intern_type(Type::Str(format!("_{i}")))).collect();
        let (elem_types, elem_labels) = (self.nested_pairs(&elems)?, self.nested_pairs(&labels)?);
        if let Some(refined) = refined_elems
            && !self.elem_types_fit(refined, elem_types)
        {
            return Err("Mirror of a tuple whose element types do not match the expected ones");
        }
        let label = match elems.len() {
            n if n <= MAX_TUPLE_ARITY => format!("Tuple{n}"),
            _ => "Tuple".to_owned(),
        };
        let label = self.u.intern_type(Type::Str(label));
        let mut core = crate::defn::type_ref(self.u, product);
        for (name, t) in [
            ("MirroredMonoType", mirrored),
            ("MirroredType", mirrored),
            ("MirroredLabel", label),
            ("MirroredElemTypes", elem_types),
            ("MirroredElemLabels", elem_labels),
        ] {
            let alias = self.u.intern_type(Type::Alias(t));
            core = self.u.intern_type(Type::Refined(core, name.to_owned(), alias));
        }
        let cast = match self.u.is_subtype(core, pt) {
            true => core,
            false => self.u.intern_type(Type::And(pt, core)),
        };
        let ctor = self.u.decl(runtime, "<init>").ok().and_then(|d| d.first().copied()).ok_or(MISSING)?;
        let raw = crate::defn::type_ref(self.u, runtime);
        let tpt = self.tpd.node(T::TypeTree { inferred: false }, at, Some(raw), None);
        let new = self.tpd.node(T::New { tpt }, at, Some(raw), None);
        let select = self.build().select(new, ctor, at);
        let arity = self.build().literal(Constant::Int(elems.len() as i32), at);
        let created = self.build().apply(select, &[arity], raw, at);
        self.cast_to(created, cast, at).map(Some).ok_or(MISSING)
    }

    /// What `pt` says as a refinement of `Mirror` or `Mirror.Product`.
    fn mirror_pt(&mut self, pt: TypeId) -> Result<MirrorPt, &'static str> {
        let mut t = self.u.dealias(pt);
        let mut found = MirrorPt { mirrored: None, elem_types: None, others: false };
        while let Type::Refined(parent, name, info) = self.u.types().get(t).clone() {
            found.others |= !KNOWN.contains(&name.as_str());
            match name.as_str() {
                "MirroredType" => {
                    found.mirrored = match self.u.types().get(info).clone() {
                        Type::Alias(a) => Some(a),
                        Type::Bounds(lo, hi) if lo == hi => Some(lo),
                        Type::Bounds(..) => return Err("Mirror of an abstract mirrored type"),
                        _ => Some(info),
                    }
                }
                "MirroredElemTypes" => found.elem_types = Some(info),
                _ => {}
            }
            t = self.u.dealias(parent);
        }
        let base = self.u.class_symbol(t).map(|c| self.u.full_name(c));
        let is_mirror =
            matches!(base.as_deref(), Some("scala.deriving.Mirror" | "scala.deriving.Mirror$.Product"));
        if !is_mirror {
            found.mirrored = None;
        }
        Ok(found)
    }

    /// Whether `MirroredElemTypes` refinement `info` admits `elems` (dotty's `checkRefinement`):
    /// an alias of the same type, or bounds around it.
    fn elem_types_fit(&mut self, info: TypeId, elems: TypeId) -> bool {
        let (lo, hi) = match self.u.types().get(info).clone() {
            Type::Alias(a) => (a, a),
            Type::Bounds(lo, hi) => (lo, hi),
            _ => (info, info),
        };
        self.u.is_subtype(lo, elems) && self.u.is_subtype(elems, hi)
    }

    /// `t1 *: t2 *: … *: EmptyTuple` (dotty's `TypeOps.nestedPairs`).
    fn nested_pairs(&mut self, elems: &[TypeId]) -> Result<TypeId, &'static str> {
        let pair = self.resolve_full_name("scala.*:", true).ok_or(MISSING)?;
        let empty = self.resolve_full_name("scala.EmptyTuple", false).ok_or(MISSING)?;
        let pair = crate::defn::type_ref(self.u, pair);
        let mut t = self.u.intern_type(Type::Sym(None, empty));
        for &e in elems.iter().rev() {
            t = self.u.intern_type(Type::Applied(pair, vec![e, t]));
        }
        Ok(t)
    }

    /// `tree.$asInstanceOf$[to]` at `at`.
    fn cast_to(&mut self, tree: TNodeId, to: TypeId, at: Span) -> Option<TNodeId> {
        let any = self.u.builtins().any;
        let m = self.u.decl(any, "$asInstanceOf$").ok()?.first().copied()?;
        let select_tpe = self.u.intern_type(Type::Sym(Some(self.tpd.tpe(tree)), m));
        let name = TName { name: self.names.intern("$asInstanceOf$"), is_type: false };
        let fun = self.tpd.node(T::Select { qualifier: tree, name }, at, Some(select_tpe), Some(m));
        let tpt = self.tpd.node(T::TypeTree { inferred: false }, at, Some(to), None);
        let args = self.tpd.list(&[tpt]);
        Some(self.tpd.node(T::TypeApply { fun, args }, at, Some(to), None))
    }
}
