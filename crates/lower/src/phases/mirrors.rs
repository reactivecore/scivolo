// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/SymUtils.scala
//   compiler/src/dotty/tools/dotc/transform/SyntheticMembers.scala
//! The mirror support dotty's `SyntheticMembers` gives objects (`addMirrorSupport`): the
//! companion of a case class extends `Mirror.Product` with `fromProduct`, a case object extends
//! `Mirror.Singleton`.

use super::anon_mirrors::anon_mirror_support;
use super::case_members::case_accessors;
use super::enum_values::is_enum_value;
use super::lift_names::fresh;
use super::sum_mirrors::is_generic_sum;
use crate::definitions::{symbol, type_ref};
use crate::symbols::has;
use crate::{Lower, Result, unsupported};
use syntax::ast::Span;
use tasty::tags::{CASE, ENUM, SEALED, SYNTHETIC};
use tasty::{Flags, Kind, SymbolId, Type, TypeId};
use typer::tpd::{TNodeId, Tree};

/// The parent object class `class` gets and its members, at `span`'s point.
pub fn mirror_support(
    l: &mut Lower,
    root: TNodeId,
    class: SymbolId,
    span: Span,
) -> Result<Option<(SymbolId, Vec<TNodeId>)>> {
    if let Some(kind) = l.tpd.anon_mirror(class) {
        return anon_mirror_support(l, root, class, kind, span).map(Some);
    }
    if is_enum_value(l, class) {
        return Ok(Some((l.case_classes()?.mirror_singleton, Vec::new())));
    }
    if !l.u.is_module_class(class) {
        return Ok(None);
    }
    if has(l.u, class, CASE) {
        return Ok(Some((l.case_classes()?.mirror_singleton, Vec::new())));
    }
    let Some(linked) = l.u.companion_class(class) else { return Ok(None) };
    if has(l.u, linked, SEALED) && has(l.u, linked, ENUM) {
        mono_type(l, class, linked);
        let focus = Span::new(span.point(), span.point(), span.point());
        let ordinal = ordinal(l, class, linked, focus)?;
        return Ok(Some((l.case_classes()?.mirror_sum, vec![ordinal])));
    }
    let focus = Span::new(span.point(), span.point(), span.point());
    if is_generic_product(l, linked) {
        mono_type(l, class, linked);
        let from_product = from_product(l, class, linked, None, focus)?;
        return Ok(Some((l.case_classes()?.mirror_product, vec![from_product])));
    }
    if !has(l.u, linked, SEALED) || !is_generic_sum(l, root, linked) {
        return Ok(None);
    }
    if !l.u.type_params(linked).is_empty() {
        return unsupported(span, "companions of generic sealed classes");
    }
    mono_type(l, class, linked);
    let param = type_ref(l.u, linked);
    let ordinal = super::sum_mirrors::ordinal(l, root, (class, linked), param, focus)?;
    Ok(Some((l.case_classes()?.mirror_sum, vec![ordinal])))
}

/// Whether `c` is a case class dotty gives a product mirror (`isGenericProduct`).
pub fn is_generic_product(l: &mut Lower, c: SymbolId) -> bool {
    let value_class = l.u.underlying(c).is_some();
    typer::sealed::is_product_shape(l.u, c, value_class)
}

/// `type MirroredMonoType = C` in object class `class` for its companion class `linked`.
fn mono_type(l: &mut Lower, class: SymbolId, linked: SymbolId) {
    let alias = symbol("MirroredMonoType", Kind::Type, class, Flags::default().with(SYNTHETIC));
    let alias = l.u.define(alias);
    let linked = type_ref(l.u, linked);
    let info = l.u.intern_type(Type::Alias(linked));
    l.u.set_info(alias, info);
}

/// `def fromProduct(x$0: Product): C = { val a$1 = x$0.productElement(0).asInstanceOf[A]; ...; new C(a$1, ...) }`
/// (dotty's `fromProductBody`), of an anonymous mirror cast to its `MirroredMonoType` `mono`, a
/// generic `C` applied to its type parameters' upper bounds.
pub fn from_product(
    l: &mut Lower,
    class: SymbolId,
    case_class: SymbolId,
    mono: Option<TypeId>,
    focus: Span,
) -> Result<TNodeId> {
    let cc = l.case_classes()?;
    let product = type_ref(l.u, cc.product);
    let result = type_ref(l.u, case_class);
    let binder = l.u.fresh_binder();
    let info = l.u.intern_type(Type::Method {
        binder,
        params: vec![("x$0".into(), product)],
        result,
        implicit: false,
        contextual: false,
    });
    let like = l.u.decl(cc.mirror_product, "fromProduct").unwrap_or_default();
    let like = like.into_iter().next().expect("`Mirror.Product.fromProduct`");
    let flags = Flags::default().with(SYNTHETIC);
    let mut locals = Vec::new();
    let (method, def) = l.method_like(class, like, flags, info, focus, |l, args, _| {
        let mut defs = Vec::new();
        let mut refs = Vec::new();
        for (i, accessor) in case_accessors(l, case_class).into_iter().enumerate() {
            let name = l.u.symbol(accessor).name.clone();
            let name = fresh(l, &name);
            let binding = l.u.define(symbol(&name, Kind::Val, class, Flags::default().with(SYNTHETIC)));
            let tpe = l.u.info(accessor);
            l.u.set_info(binding, tpe);
            locals.push(binding);
            let product_ref = l.copy_ident(args[0]);
            let index = l.int_literal(i as i32);
            let element = l.call(product_ref, cc.product_element, &[index]);
            let rhs = l.ensure_conforms(element, tpe);
            let tpt = l.type_tree(tpe, Span::NONE);
            let term = l.term_name(&name);
            let def = l.tpd.node(
                Tree::ValDef { name: term, tpt, rhs: Some(rhs) },
                Span::NONE,
                Some(tpe),
                Some(binding),
            );
            defs.push(l.positioned(def));
            refs.push(l.ident(binding, Span::NONE));
        }
        let new = l.new_(case_class, Span::NONE);
        let ctor = l.u.decl(case_class, "<init>").unwrap_or_default();
        let ctor = ctor.into_iter().next().expect("a case class constructor");
        let created = l.call(new, ctor, &refs);
        let params = l.u.type_params(case_class);
        let tpe = match mono.filter(|_| !params.is_empty()) {
            Some(_) => {
                let his = params.iter().map(|&p| typer::sealed::upper_bound(l.u, p)).collect();
                crate::definitions::applied(l.u, case_class, his)
            }
            None => result,
        };
        l.tpd.get_mut(created).tpe = Some(tpe);
        let block = l.block(&defs, created, Span::NONE);
        let block = l.positioned(block);
        match mono {
            Some(m) => l.ensure_conforms(block, m),
            None => block,
        }
    })?;
    for local in locals {
        l.u.set_owner(local, method);
    }
    l.tpd.get_mut(def).span = Span::NONE;
    Ok(l.with_span(def, focus))
}

/// `def ordinal(x$0: E): Int = x$0.ordinal` for enum `enum_class` (dotty's `ordinalBody`).
pub fn ordinal(l: &mut Lower, class: SymbolId, enum_class: SymbolId, focus: Span) -> Result<TNodeId> {
    let cc = l.case_classes()?;
    let param = type_ref(l.u, enum_class);
    let int = l.u.class_ref("scala.Int");
    let binder = l.u.fresh_binder();
    let params = vec![("x$0".into(), param)];
    let info =
        l.u.intern_type(Type::Method { binder, params, result: int, implicit: false, contextual: false });
    let like = l.u.decl(cc.mirror_sum, "ordinal").unwrap_or_default();
    let like = like.into_iter().next().expect("`Mirror.Sum.ordinal`");
    let flags = Flags::default().with(SYNTHETIC);
    let (_, def) = l.method_like(class, like, flags, info, focus, |l, args, _| {
        let x = l.copy_ident(args[0]);
        l.call(x, cc.enum_ordinal, &[])
    })?;
    l.tpd.get_mut(def).span = Span::NONE;
    Ok(l.with_span(def, focus))
}

impl Lower<'_> {
    /// `t`, cast to `tpe` unless its type conforms (dotty's `ensureConforms`).
    pub fn ensure_conforms(&mut self, t: TNodeId, tpe: tasty::TypeId) -> TNodeId {
        let from = self.tpd.tpe(t);
        if self.u.is_subtype(from, tpe) {
            return t;
        }
        let fun = self.select(t, self.defs.as_instance_of, Span::NONE);
        let fun = self.positioned(fun);
        let tpt = self.type_tree(tpe, Span::NONE);
        let args = self.tpd.list(&[tpt]);
        let cast = self.tpd.node(
            Tree::TypeApply { fun, args },
            Span::NONE,
            Some(tpe),
            Some(self.defs.as_instance_of),
        );
        self.positioned(cast)
    }
}
