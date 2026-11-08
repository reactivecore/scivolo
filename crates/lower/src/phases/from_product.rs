// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/transform/SyntheticMembers.scala
//   compiler/src/dotty/tools/dotc/ast/tpd.scala

use super::case_members::case_accessors;
use super::lift_names::fresh;
use super::mirror_prefixes::new_seen_from;
use super::mirrors::has_mirror_method;
use crate::definitions::{symbol, type_ref};
use crate::{Lower, Result, unsupported};
use syntax::ast::Span;
use tasty::tags::SYNTHETIC;
use tasty::{Flags, Kind, SymbolId, Type, TypeId};
use typer::tpd::{TNodeId, Tree};

/// `def fromProduct(x$0: Product): C = { val a$1 = x$0.productElement(0).asInstanceOf[A]; ...; new C(a$1, ...) }`
/// (dotty's `fromProductBody`), of an anonymous mirror cast to its `MirroredMonoType` `mono`, a
/// generic `C` applied to its type parameters' upper bounds; none if `class` has one. From the
/// first `@unroll` parameter on, an element is `if arity > i then x$0.productElement(i) else
/// C.<init>$default$(i+1)`, after `val arity = x$0.productArity`.
pub fn from_product(
    l: &mut Lower,
    class: SymbolId,
    case_class: SymbolId,
    mono: Option<TypeId>,
    focus: Span,
) -> Result<Option<TNodeId>> {
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
    if has_mirror_method(l, class, like, info) {
        return Ok(None);
    }
    let flags = Flags::default().with(SYNTHETIC);
    let mut locals = Vec::new();
    let accessors = case_accessors(l, case_class);
    let (module, getters) = unrolled_getters(l, case_class, &accessors)?;
    let unrolled_from = accessors.len() - getters.len();
    let (method, def) = l.method_like(class, like, flags, info, focus, |l, args, _| {
        let mut defs = Vec::new();
        let mut refs = Vec::new();
        let mut arity = None;
        if !getters.is_empty() {
            let product_ref = l.copy_ident(args[0]);
            let rhs = l.select(product_ref, cc.product_arity, Span::NONE);
            let rhs = l.positioned(rhs);
            let int = l.u.widen(l.tpd.tpe(rhs));
            let (def, binding) = val_def(l, class, "arity", int, rhs);
            locals.push(binding);
            defs.push(def);
            arity = Some(binding);
        }
        for (i, accessor) in accessors.iter().copied().enumerate() {
            let tpe = l.u.info(accessor);
            let product_ref = l.copy_ident(args[0]);
            let index = l.int_literal(i as i32);
            let mut element = l.call(product_ref, cc.product_element, &[index]);
            if let Some(arity) = arity.filter(|_| i >= unrolled_from) {
                let (arity, index) = (l.ident(arity, Span::NONE), l.int_literal(i as i32));
                let gt = l.operator(arity, ">", index).expect("`Int.>(Int)`");
                let cond = l.call(arity, gt, &[index]);
                let module = l.ident(module, Span::NONE);
                let default = l.select(module, getters[i - unrolled_from], Span::NONE);
                let elsep = l.positioned(default);
                let branch = Tree::If { cond, thenp: element, elsep, inline: false };
                element = l.tpd.node(branch, Span::NONE, Some(tasty::ANY), None);
                element = l.positioned(element);
            }
            let rhs = l.ensure_conforms(element, tpe);
            let name = l.u.symbol(accessor).name.clone();
            let name = fresh(l, &name);
            let (def, binding) = val_def(l, class, &name, tpe, rhs);
            locals.push(binding);
            defs.push(def);
            refs.push(l.ident(binding, Span::NONE));
        }
        let new = new_seen_from(l, case_class, mono);
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
    Ok(Some(l.with_span(def, focus)))
}

/// `val name: tpe = rhs`, its symbol owned by `class` until the caller moves it.
fn val_def(l: &mut Lower, class: SymbolId, name: &str, tpe: TypeId, rhs: TNodeId) -> (TNodeId, SymbolId) {
    let binding = l.u.define(symbol(name, Kind::Val, class, Flags::default().with(SYNTHETIC)));
    l.u.set_info(binding, tpe);
    let tpt = l.type_tree(tpe, Span::NONE);
    let term = l.term_name(name);
    let def =
        l.tpd.node(Tree::ValDef { name: term, tpt, rhs: Some(rhs) }, Span::NONE, Some(tpe), Some(binding));
    (l.positioned(def), binding)
}

/// The companion of `case_class` and the default getters of its case accessors `accessors` from
/// the first `@unroll` one on; none without one.
fn unrolled_getters(
    l: &mut Lower,
    case_class: SymbolId,
    accessors: &[SymbolId],
) -> Result<(SymbolId, Vec<SymbolId>)> {
    let unroll = |l: &Lower, a: SymbolId| {
        l.u.annotations(a).iter().any(|&c| l.u.full_name(c) == "scala.annotation.unroll")
    };
    let first = (0..accessors.len()).find(|&i| unroll(l, accessors[i]));
    let module = l.u.companion_module(case_class);
    let (Some(first), Some(module)) = (first, module) else { return Ok((case_class, Vec::new())) };
    let class = l.u.module_class(module).unwrap_or(module);
    let mut getters = Vec::new();
    for n in first..accessors.len() {
        let name = format!("<init>$default${}", n + 1);
        let getter = l.u.decl(class, &name).unwrap_or_default().into_iter().next();
        let info = getter.map(|g| l.u.info(g));
        let plain =
            info.is_some_and(|i| !matches!(l.u.types().get(i), Type::Poly { .. } | Type::Method { .. }));
        let Some(g) = getter.filter(|_| plain) else {
            return unsupported(
                Span::NONE,
                "fromProduct of @unroll parameters without plain default getters",
            );
        };
        getters.push(g);
    }
    Ok((module, getters))
}

impl Lower<'_> {
    /// `t`, cast to `tpe` unless its type conforms (dotty's `ensureConforms`).
    pub fn ensure_conforms(&mut self, t: TNodeId, tpe: TypeId) -> TNodeId {
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
