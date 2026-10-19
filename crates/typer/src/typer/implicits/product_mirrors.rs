// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/SymUtils.scala
//   compiler/src/dotty/tools/dotc/typer/Synthesizer.scala
//! A `Mirror` search (dotty's `synthesizedMirror`), and the product mirrors of case classes and
//! singletons (`productMirror`): the companion, the singleton or an anonymous class, cast to the
//! mirror type.

use super::super::Typer;
use super::mirror_sources::MirrorSource;
use super::mirror_types::{MirrorCore, MirrorFormal, MirrorKind};
use crate::ctx::Ctx;
use crate::tpd::TNodeId;
use syntax::ast::Span;
use tasty::tags::{CASE, CASEaccessor, PRIVATE, PROTECTED};
use tasty::{Kind, SymbolId, Type, TypeId};

impl Typer<'_> {
    /// The mirror `pt` asks for, if it is a `Mirror` type whose mirrored type has one.
    pub fn synthesized_mirror(
        &mut self,
        pt: TypeId,
        at: Span,
        ctx: Ctx,
    ) -> Result<Option<TNodeId>, &'static str> {
        let Some(mut formal) = self.mirror_formal(pt) else { return Ok(None) };
        let mirrored = self.zonk(formal.mirrored);
        if !self.is_fully_defined(mirrored) {
            return Err("Mirror of a type with type variables");
        }
        formal.mirrored = mirrored;
        let Some(source) = self.mirror_source(mirrored)? else { return Ok(None) };
        match formal.kind {
            MirrorKind::Product => self.product_mirror(&formal, source, at, ctx),
            MirrorKind::Sum => self.sum_mirror(&formal, source, at, ctx),
            MirrorKind::Any => match self.product_mirror(&formal, source.clone(), at, ctx) {
                Ok(Some(m)) => Ok(Some(m)),
                Ok(None) => self.sum_mirror(&formal, source, at, ctx),
                Err(what) => match self.sum_mirror(&formal, source, at, ctx) {
                    Ok(Some(m)) => Ok(Some(m)),
                    _ => Err(what),
                },
            },
        }
    }

    fn product_mirror(
        &mut self,
        formal: &MirrorFormal,
        source: MirrorSource,
        at: Span,
        ctx: Ctx,
    ) -> Result<Option<TNodeId>, &'static str> {
        let (pre, cls) = match source {
            MirrorSource::Tuple(elems) => return self.tuple_mirror(formal, &elems, at),
            MirrorSource::Singleton(s, path) => return self.singleton_mirror(formal, s, path, at).map(Some),
            MirrorSource::Class(pre, cls) => (pre, cls),
        };
        if let Some(args) = self.tuple_class_args(formal.mirrored).filter(|a| !a.is_empty()) {
            return self.tuple_mirror(formal, &args, at);
        }
        if !self.is_generic_product(cls, ctx) {
            return Ok(None);
        }
        let accessors = self.case_accessors(cls);
        if accessors.iter().any(|&a| self.is_unrolled(a)) {
            return Err("Mirror of a case class with @unroll parameters");
        }
        let labels: Vec<String> = accessors.iter().map(|&a| self.u.symbol(a).name.clone()).collect();
        let site = match self.u.types().get(formal.mirrored).clone() {
            Type::Lambda { result, .. } => result,
            _ => formal.mirrored,
        };
        let elems: Vec<TypeId> = accessors
            .iter()
            .map(|&a| {
                let t = self.u.member_info(site, a);
                match *self.u.types().get(t) {
                    Type::ByName(r) => r,
                    _ => t,
                }
            })
            .collect();
        let nested = self.nested_pairs(&elems)?;
        let (mono, elem_types) = self.mono_and_elems(formal.mirrored, nested);
        let elem_labels = self.label_pairs(&labels)?;
        let label = self.u.symbol(cls).name.clone();
        let core = MirrorCore {
            parent: "scala.deriving.Mirror$.Product",
            mono,
            label: &label,
            elems: Some((elem_types, elem_labels)),
        };
        let cast = self.mirror_type(formal, core)?;
        let mirror = match self.product_companion(cls) {
            Some(module) => {
                let path = self.u.intern_type(Type::Sym(pre, module));
                let tree = self.path_tree(path, at, 8).ok_or("Mirror companion outside the subset")?;
                let class = self.u.module_class(module).filter(|&c| self.encloses(c, ctx.owner));
                match class {
                    Some(class) if matches!(self.tpd.tree(tree), crate::tpd::Tree::Select { .. }) => {
                        let class_type = crate::defn::type_ref(self.u, class);
                        self.build().this(class_type, at)
                    }
                    _ => tree,
                }
            }
            None => self.anonymous_mirror(mono, false, at, ctx)?,
        };
        self.cast_to(mirror, cast, at).map(Some)
    }

    /// The case object or enum value `s` at `path` cast to its `Mirror.Singleton` type; a
    /// Scala 2 object wrapped in `new Mirror.SingletonProxy(s)`.
    fn singleton_mirror(
        &mut self,
        formal: &MirrorFormal,
        s: SymbolId,
        path: TypeId,
        at: Span,
    ) -> Result<TNodeId, &'static str> {
        let empty = self.nested_pairs(&[])?;
        self.check_refinement(formal.pt, "MirroredElemTypes", empty)?;
        self.check_refinement(formal.pt, "MirroredElemLabels", empty)?;
        let scala2 = self.u.is_module(s) && self.u.module_class(s).is_some_and(|c| self.u.is_scala2(c));
        let parent = match scala2 {
            true => "scala.deriving.Mirror$.SingletonProxy",
            false => "scala.deriving.Mirror$.Singleton",
        };
        let label = self.u.symbol(s).name.clone();
        let core = MirrorCore { parent, mono: formal.mirrored, label: &label, elems: None };
        let cast = self.mirror_type(formal, core)?;
        let mut value = self.path_tree(path, at, 8).ok_or("Mirror of a singleton outside the subset")?;
        if scala2 {
            value = self.new_with_args(parent, &[value], at)?;
        }
        self.cast_to(value, cast, at)
    }

    /// Whether `c` is a case class with a product mirror in `ctx` (dotty's `isGenericProduct`):
    /// its companion the mirror or its constructor accessible.
    pub fn is_generic_product(&mut self, c: SymbolId, ctx: Ctx) -> bool {
        self.complete(c);
        if let Some(&ctor) = self.u.decl(c, "<init>").unwrap_or_default().first() {
            self.complete(ctor);
        }
        let value_class = self.is_derived_value_class(c);
        crate::sealed::is_product_shape(self.u, c, value_class)
            && (self.product_companion(c).is_some() || self.can_access_constructor(c, ctx))
    }

    /// Whether the primary constructor of `c` is accessible from `ctx` for an anonymous mirror
    /// (dotty's `canAccessCtor`).
    fn can_access_constructor(&mut self, c: SymbolId, ctx: Ctx) -> bool {
        let Some(&ctor) = self.u.decl(c, "<init>").unwrap_or_default().first() else { return true };
        let flags = self.u.symbol(ctor).flags;
        let owners: Vec<SymbolId> =
            std::iter::successors(Some(ctx.owner), |&o| self.u.symbol(o).owner).collect();
        let sub = owners.iter().any(|&o| self.u.is_class(o) && self.u.derives_from(o, c));
        let within = self.syms.private_within.get(&ctor).is_none_or(|w| owners.contains(w));
        (!(flags.has(PRIVATE) || flags.has(PROTECTED)) || sub) && within
    }

    /// The case accessors of class `c`, in order (dotty's `caseAccessors`).
    pub fn case_accessors(&mut self, c: SymbolId) -> Vec<SymbolId> {
        self.complete(c);
        let decls = self.u.decls(c).unwrap_or_default();
        let accessor = |t: &Self, d: SymbolId| {
            matches!(t.u.symbol(d).kind, Kind::Val | Kind::Param) && t.u.symbol(d).flags.has(CASEaccessor)
        };
        decls.into_iter().filter(|&d| accessor(self, d)).collect()
    }

    /// Whether `s` is annotated `@unroll`.
    fn is_unrolled(&self, s: SymbolId) -> bool {
        self.u.annotations(s).iter().any(|&a| self.u.full_name(a) == "scala.annotation.unroll")
    }

    /// Whether `owner` is `class` or owned by it, transitively.
    fn encloses(&self, class: SymbolId, owner: SymbolId) -> bool {
        std::iter::successors(Some(owner), |&o| self.u.symbol(o).owner).any(|o| o == class)
    }

    /// The companion of class `c` if it is its mirror (dotty's `useCompanionAsProductMirror`).
    fn product_companion(&mut self, c: SymbolId) -> Option<SymbolId> {
        if self.u.is_scala2(c) {
            return None;
        }
        let module = self.u.companion_module(c)?;
        let case = self.u.module_class(module).is_some_and(|m| self.u.symbol(m).flags.has(CASE));
        (!case).then_some(module)
    }
}
