// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/transform/SyntheticMembers.scala
//   compiler/src/dotty/tools/dotc/ast/tpd.scala

use super::case_members::case_accessors;
use super::lift_names::fresh;
use super::mirror_prefixes::new_seen_from;
use super::mirrors::has_mirror_method;
use crate::definitions::{symbol, type_ref};
use crate::{Lower, Result};
use syntax::ast::Span;
use tasty::tags::SYNTHETIC;
use tasty::{Flags, Kind, SymbolId, Type, TypeId};
use typer::tpd::{TNodeId, Tree};

/// `def fromProduct(x$0: Product): M = { val a$1 = x$0.productElement(0).asInstanceOf[A]; ...; new C[Ts](a$1, ...) }`
/// (dotty's `fromProductBody`), `M` the `MirroredMonoType` `mono`, a generic `C` applied to its
/// type parameters' upper bounds; none if `class` has one.
pub fn from_product(
    l: &mut Lower,
    class: SymbolId,
    case_class: SymbolId,
    mono: TypeId,
    focus: Span,
) -> Result<Option<TNodeId>> {
    let cc = l.case_classes()?;
    let product = type_ref(l.u, cc.product);
    let binder = l.u.fresh_binder();
    let info = l.u.intern_type(Type::Method {
        binder,
        params: vec![("x$0".into(), product)],
        result: mono,
        implicit: false,
        contextual: false,
    });
    let like = l.u.decl(cc.mirror_product, "fromProduct").unwrap_or_default();
    let like = like.into_iter().next().expect("`Mirror.Product.fromProduct`");
    if has_mirror_method(l, class, like, info) {
        return Ok(None);
    }
    let ctor = l.u.decl(case_class, "<init>").unwrap_or_default();
    let ctor = ctor.into_iter().next().expect("a case class constructor");
    let (created_type, param_types) = instantiated_constructor(l, case_class, ctor);
    let flags = Flags::default().with(SYNTHETIC);
    let mut locals = Vec::new();
    let (method, def) = l.method_like(class, like, flags, info, focus, |l, args, _| {
        let mut defs = Vec::new();
        let mut refs = Vec::new();
        let accessors = case_accessors(l, case_class);
        for (i, (accessor, param)) in accessors.into_iter().zip(param_types).enumerate() {
            let repeated = repeated_elem(l, param);
            let tpe = match repeated {
                Some(elem) => {
                    let seq = l.u.class_ref("scala.collection.immutable.Seq");
                    l.u.intern_type(Type::Applied(seq, vec![elem]))
                }
                None => param,
            };
            let name = l.u.symbol(accessor).name.clone();
            let name = fresh(l, &name);
            let binding = l.u.define(symbol(&name, Kind::Val, class, Flags::default().with(SYNTHETIC)));
            l.u.set_info(binding, tpe);
            locals.push(binding);
            let product_ref = l.copy_ident(args[0]);
            let index = l.int_literal(i as i32);
            let element = l.call(product_ref, cc.product_element, &[index]);
            let rhs = l.type_cast_unless_conforms(element, tpe);
            let tpt = l.type_tree(tpe, Span::NONE);
            let term = l.term_name(&name);
            let def = l.tpd.node(
                Tree::ValDef { name: term, tpt, rhs: Some(rhs) },
                Span::NONE,
                Some(tpe),
                Some(binding),
            );
            defs.push(l.positioned(def));
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
    for local in locals {
        l.u.set_owner(local, method);
    }
    l.tpd.get_mut(def).span = Span::NONE;
    Ok(Some(l.with_span(def, focus)))
}

/// The type `new C[Ts]` creates and the types of the parameters of constructor `ctor` of case
/// class `c`, `Ts` the upper bounds of `c`'s type parameters (dotty's constructor type variables
/// instantiated from above).
fn instantiated_constructor(l: &mut Lower, c: SymbolId, ctor: SymbolId) -> (TypeId, Vec<TypeId>) {
    let accessors = case_accessors(l, c);
    let info = l.u.info(ctor);
    let Type::Poly { binder, params, result } = l.u.types().get(info).clone() else {
        let types = accessors.into_iter().map(|a| l.u.info(a)).collect();
        return (type_ref(l.u, c), types);
    };
    let names: Vec<String> = params.iter().map(|(n, _)| n.clone()).collect();
    let his: Vec<TypeId> =
        l.u.type_params(c).into_iter().map(|p| typer::sealed::upper_bound(l.u, p)).collect();
    let method = l.u.subst(result, binder, &names, &his);
    let types = match l.u.types().get(method).clone() {
        Type::Method { params, .. } => params.into_iter().map(|(_, t)| t).collect(),
        _ => accessors.into_iter().map(|a| l.u.info(a)).collect(),
    };
    (crate::definitions::applied(l.u, c, his), types)
}

/// The element type `T` of repeated parameter type `<repeated>[T]`.
fn repeated_elem(l: &mut Lower, t: TypeId) -> Option<TypeId> {
    let Type::Applied(c, args) = l.u.types().get(t).clone() else { return None };
    let class = l.u.class_symbol(c)?;
    (args.len() == 1 && l.u.full_name(class) == "scala.<repeated>").then(|| args[0])
}

impl Lower<'_> {
    /// `t`, cast to `tpe` unless its type conforms.
    pub fn ensure_conforms(&mut self, t: TNodeId, tpe: TypeId) -> TNodeId {
        self.cast_unless_conforms(t, tpe, self.defs.as_instance_of)
    }

    /// `t`, cast to `tpe` with `$asInstanceOf$` unless its type conforms (dotty's
    /// `ensureConforms` before erasure).
    pub fn type_cast_unless_conforms(&mut self, t: TNodeId, tpe: TypeId) -> TNodeId {
        self.cast_unless_conforms(t, tpe, self.defs.type_cast)
    }

    fn cast_unless_conforms(&mut self, t: TNodeId, tpe: TypeId, cast: SymbolId) -> TNodeId {
        let from = self.tpd.tpe(t);
        if self.u.is_subtype(from, tpe) {
            return t;
        }
        let fun = self.select(t, cast, Span::NONE);
        let fun = self.positioned(fun);
        let tpt = self.type_tree(tpe, Span::NONE);
        let args = self.tpd.list(&[tpt]);
        let cast = self.tpd.node(Tree::TypeApply { fun, args }, Span::NONE, Some(tpe), Some(cast));
        self.positioned(cast)
    }
}
