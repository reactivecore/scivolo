// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/transform/SyntheticMembers.scala
//   compiler/src/dotty/tools/dotc/ast/tpd.scala

use super::case_members::case_accessors;
use super::constructor_instance::{instantiated_constructor, repeated_elem};
use super::lift_names::fresh;
use super::mirror_prefixes::new_seen_from;
use super::mirrors::has_mirror_method;
use super::synthetic::pickled_ref;
use crate::definitions::symbol;
use crate::{Lower, Result, unsupported};
use names::{name_kinds, nme};
use syntax::ast::Span;
use tasty::tags::SYNTHETIC;
use tasty::{Flags, Kind, SymbolId, Type, TypeId};
use typer::tpd::{TNodeId, Tree};

/// `def fromProduct(x$0: Product): M = { val a$1 = x$0.productElement(0).asInstanceOf[A]; ...; new C[Ts](a$1, ...) }`
/// (dotty's `fromProductBody`), `M` the `MirroredMonoType` `mono`, a generic `C` applied to its
/// type parameters' upper bounds; none if `class` has one. From the first `@unroll` parameter
/// on, an element is `if arity > i then x$0.productElement(i) else C.<init>$default$(i+1)`, after
/// `val arity = x$0.productArity`.
pub fn from_product(
    l: &mut Lower,
    class: SymbolId,
    case_class: SymbolId,
    mono: TypeId,
    focus: Span,
) -> Result<Option<TNodeId>> {
    let cc = l.case_classes()?;
    let product = pickled_ref(l, cc.product);
    let binder = l.u.fresh_binder();
    let x0 = l.u.intern_name(&name_kinds::synthetic_param(0));
    let info = l.u.intern_method(binder, &[(x0, product)], mono, false, false);
    let like = l.u.decls_named(cc.mirror_product, nme::FROM_PRODUCT);
    let like = like.into_iter().next().expect("`Mirror.Product.fromProduct`");
    if has_mirror_method(l, class, like, info) {
        return Ok(None);
    }
    let ctor = l.u.decls_named(case_class, nme::CONSTRUCTOR);
    let ctor = ctor.into_iter().next().expect("a case class constructor");
    let (created_type, param_types) = instantiated_constructor(l, case_class, ctor);
    if param_types.iter().any(|&p| repeated_elem(l, p).is_none() && l.u.repeated_elem(p).is_some()) {
        return unsupported(focus, "fromProduct of a repeated parameter of an annotated type");
    }
    let flags = Flags::default().with(SYNTHETIC);
    let accessors = case_accessors(l, case_class);
    let (module, getters) = unrolled_getters(l, case_class, &accessors)?;
    let unrolled_from = accessors.len() - getters.len();
    let bindings = bindings(l, class, ctor, &accessors, &param_types);
    let (_, def) = l.method_like(class, like, flags, info, focus, |l, args, _| {
        let method = l.tpd.sym(args[0]).and_then(|p| l.u.symbol(p).owner).expect("`fromProduct`");
        for &binding in &bindings {
            l.u.set_owner(binding, method);
        }

        let mut defs = Vec::new();
        let mut refs = Vec::new();
        let mut arity = None;
        if !getters.is_empty() {
            let product_ref = l.copy_ident(args[0]);
            l.tpd.mark_copy(product_ref, args[0]);
            let rhs = l.select(product_ref, cc.product_arity, Span::NONE);
            let rhs = l.positioned(rhs);
            let int = l.u.widen(l.tpd.tpe(rhs));
            let binding = val_sym(l, class, nme::ARITY, int);
            l.u.set_owner(binding, method);
            defs.push(val_def(l, binding, rhs));
            arity = Some(binding);
        }
        for (i, (&binding, &param)) in bindings.iter().zip(&param_types).enumerate() {
            let repeated = repeated_elem(l, param);
            let product_ref = l.copy_ident(args[0]);
            l.tpd.mark_copy(product_ref, args[0]);
            let index = l.int_literal(i as i32);
            let mut element = l.call(product_ref, cc.product_element, &[index]);
            if let Some(arity) = arity.filter(|_| i >= unrolled_from) {
                let (arity, index) = (l.ident(arity, Span::NONE), l.int_literal(i as i32));
                let gt = l.operator(arity, nme::GT, index).expect("`Int.>(Int)`");
                let cond = l.call(arity, gt, &[index]);
                let module = l.ident(module, Span::NONE);
                let default = l.select(module, getters[i - unrolled_from], Span::NONE);
                let elsep = l.positioned(default);
                let branch = Tree::If { cond, thenp: element, elsep, inline: false };
                element = l.tpd.node(branch, Span::NONE, Some(tasty::ANY), None);
                element = l.positioned(element);
            }
            let tpe = l.u.info(binding);
            let rhs = l.type_cast_unless_conforms(element, tpe);
            defs.push(val_def(l, binding, rhs));
            let arg = l.ident(binding, Span::NONE);
            refs.push(match repeated {
                Some(_) => {
                    let tpt = l.type_tree(param, Span::NONE);
                    l.tpd.node(Tree::Typed { expr: arg, tpt }, Span::NONE, Some(param), None)
                }
                None => arg,
            });
        }
        let new = new_seen_from(l, case_class, Some(mono));
        let created = l.call(new, ctor, &refs);
        l.tpd.get_mut(created).tpe = Some(created_type);
        let block = l.block(&defs, created, Span::NONE);
        let block = l.positioned(block);
        l.type_cast_unless_conforms(block, mono)
    })?;
    l.tpd.get_mut(def).span = Span::NONE;
    Ok(Some(l.with_span(def, focus)))
}

/// The synthetic `val name: tpe`, owned by `class` until the caller moves it.
fn val_sym(l: &mut Lower, class: SymbolId, name: &str, tpe: TypeId) -> SymbolId {
    let binding = l.u.define(symbol(name, Kind::Val, class, Flags::default().with(SYNTHETIC)));
    l.u.set_info(binding, tpe);
    binding
}

/// `val b: T = rhs` of `val` `binding` of type `T`.
fn val_def(l: &mut Lower, binding: SymbolId, rhs: TNodeId) -> TNodeId {
    let tpe = l.u.info(binding);
    let tpt = l.type_tree(tpe, Span::NONE);
    let term = l.term_name(&l.u.symbol(binding).name.clone());
    let def =
        l.tpd.node(Tree::ValDef { name: term, tpt, rhs: Some(rhs) }, Span::NONE, Some(tpe), Some(binding));
    l.positioned(def)
}

/// The `val`s of `fromProduct` for case accessors `accessors` of a class with constructor `ctor`,
/// of parameter types `params`, a repeated one's as a `Seq`, owned by `class` until the caller
/// moves them; a reference to a constructor parameter or its accessor on `this` names its `val`
/// instead (dotty's `substParams` of a dependent constructor): `val b$1: B^{a$1}` of
/// `case class C(a: A^, b: B^{a})`.
fn bindings(
    l: &mut Lower,
    class: SymbolId,
    ctor: SymbolId,
    accessors: &[SymbolId],
    params: &[TypeId],
) -> Vec<SymbolId> {
    let mut vals = Vec::new();
    let mut originals = Vec::new();
    for (&accessor, &param) in accessors.iter().zip(params) {
        let tpe = match repeated_elem(l, param) {
            Some(elem) => {
                let seq = l.u.class_ref(tasty::defn::IMMUTABLE_SEQ);
                l.u.intern_applied(seq, &[elem])
            }
            None => param,
        };
        let name = l.u.symbol(accessor).name.clone();
        let original = l.u.decls_named(ctor, &name).into_iter().find(|&p| l.u.symbol(p).kind == Kind::Param);
        originals.push(original);
        let name = fresh(l, class, &name);
        vals.push(val_sym(l, class, &name, tpe));
    }

    for &val in &vals {
        let info = l.u.info(val);
        let info = l.u.map_type_and_kept_sets(info, &mut |u, part| {
            let Type::Sym(prefix, r) = *u.ty(part) else { return None };
            if prefix.is_some_and(|p| !matches!(u.ty(p), Type::This(_))) {
                return None;
            }
            let i = accessors.iter().position(|&a| a == r);
            let i = i.or_else(|| originals.iter().position(|&p| p == Some(r)))?;
            Some(u.intern_type(Type::Sym(None, vals[i])))
        });
        l.u.set_info(val, info);
    }
    vals
}

/// The companion of `case_class` and the default getters of its case accessors `accessors` from
/// the first `@unroll` one on; none without one.
fn unrolled_getters(
    l: &mut Lower,
    case_class: SymbolId,
    accessors: &[SymbolId],
) -> Result<(SymbolId, Vec<SymbolId>)> {
    let unroll = |l: &Lower, a: SymbolId| l.u.has_recorded_annotation(a, tasty::defn::UNROLL);
    let first = (0..accessors.len()).find(|&i| unroll(l, accessors[i]));
    let module = l.u.companion_module(case_class);
    let (Some(first), Some(module)) = (first, module) else { return Ok((case_class, Vec::new())) };
    let class = l.u.module_class(module).unwrap_or(module);
    let mut getters = Vec::new();
    for n in first..accessors.len() {
        let name = name_kinds::default_getter(nme::CONSTRUCTOR, n + 1);
        let getter = l.u.decls_named(class, &name).into_iter().next();
        let info = getter.map(|g| l.u.info(g));
        let plain = info.is_some_and(|i| !matches!(l.u.ty(i), Type::Poly { .. } | Type::Method { .. }));
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
