// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/ast/tpd.scala
//   compiler/src/dotty/tools/dotc/inlines/Inlines.scala
//! `constValueTuple` and `summonAll` (dotty's `InlineCall.expand` intrinsics with
//! `unrollTupleTypes` and `tupleTree`).

use super::{Inlining, Unsupported};
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::Span;
use tasty::{Binding, Kind, Type, TypeId};

/// The largest arity of a `TupleN` class.
const MAX_TUPLE_ARITY: usize = 22;
/// The largest number of tuple elements unrolled.
const MAX_UNROLLED: usize = 1024;

impl Inlining<'_> {
    /// `constValueTuple[t]` or `summonAll[t]` (`summon`) as a tuple of the constants or
    /// implicits of `t`'s element types, typed as `t`.
    pub fn tuple_intrinsic(
        &mut self,
        targ: TNodeId,
        summon: bool,
        span: Span,
    ) -> Result<TNodeId, Unsupported> {
        let tpe = self.tpd.tpe(targ);
        let unknown = (span, "tuple element types not known at compile time");
        let types = self.unrolled_tuple(tpe, MAX_UNROLLED).ok_or(unknown)?;
        let mut elems = Vec::new();
        for t in types {
            elems.push(match summon {
                true => {
                    let at = self.tpd.span(targ).start_pos();
                    self.summon(t, at)?.ok_or((span, "no implicit argument"))?
                }
                false => {
                    let value =
                        self.const_value(t).ok_or((span, "constValue of a type that is not constant"))?;
                    self.literal(value, span)
                }
            });
        }
        let tuple = self.tuple_tree(&elems, span)?;
        let tpt = self.tpd.node(T::TypeTree { inferred: false }, span, Some(tpe), None);
        Ok(self.tpd.node(T::Typed { expr: tuple, tpt }, span, Some(tpe), None))
    }

    /// The element types of tuple type `t`: a `TupleN`, a `*:` chain ending in `EmptyTuple`, or a
    /// match type reducing to one.
    fn unrolled_tuple(&mut self, t: TypeId, depth: usize) -> Option<Vec<TypeId>> {
        let depth = depth.checked_sub(1)?;
        let d = self.u.dealias(t);
        if let Type::Match { .. } = self.u.types().get(d) {
            let n = self.u.try_normalize(d)?;
            return self.unrolled_tuple(n, depth);
        }
        if let Type::Applied(tycon, args) = self.u.types().get(d).clone() {
            let name = self.u.class_symbol(tycon).map(|c| self.u.full_name(c));
            return match name.as_deref() {
                Some(n) if n == format!("scala.Tuple{}", args.len()) => Some(args),
                Some("scala.*:") if args.len() == 2 => {
                    let mut elems = vec![args[0]];
                    elems.extend(self.unrolled_tuple(args[1], depth)?);
                    Some(elems)
                }
                _ => {
                    let n = self.u.try_normalize(d)?;
                    self.unrolled_tuple(n, depth)
                }
            };
        }
        let widened = self.u.widen(d);
        let class = self.u.class_symbol(widened)?;
        let empty =
            self.u.symbol(class).name == "EmptyTuple$" && self.u.full_name(class).starts_with("scala.");
        empty.then(Vec::new)
    }

    /// `EmptyTuple`, `TupleN.apply[T1, …, Tn](elems)` with the types of `elems` for 1 to 22
    /// elements, else `TupleXXL.apply(elems*)` cast to their `*:` chain (dotty's `tupleTree`).
    fn tuple_tree(&mut self, elems: &[TNodeId], span: Span) -> Result<TNodeId, Unsupported> {
        let missing = (span, "TupleN");
        if elems.is_empty() {
            let empty = self.scala_member("EmptyTuple", Kind::Val).ok_or(missing)?;
            let empty = crate::defn::type_ref(self.u, empty);
            return self.singleton(empty, span).ok_or(missing);
        }
        if elems.len() > MAX_TUPLE_ARITY {
            return self.tuple_xxl(elems, span);
        }
        let module = self.scala_member(&format!("Tuple{}", elems.len()), Kind::Val).ok_or(missing)?;
        let module_class = self.u.module_class(module).ok_or(missing)?;
        let apply =
            self.u.decl(module_class, "apply").ok().and_then(|d| d.first().copied()).ok_or(missing)?;
        let types: Vec<TypeId> = elems.iter().map(|&e| self.tpd.tpe(e)).collect();
        let at = span.start_pos();
        let module_ref = crate::defn::type_ref(self.u, module);
        let qualifier = self.singleton(module_ref, at).ok_or(missing)?;
        let apply_ref = self.u.intern_type(Type::Sym(Some(module_ref), apply));
        let name = self.term_name(apply);
        let select = self.tpd.node(T::Select { qualifier, name }, at, Some(apply_ref), Some(apply));
        let info = self.u.info(apply);
        let Some(Binding { binder, params, result, .. }) = self.u.poly(info) else {
            return Err(missing);
        };
        let names: Vec<String> = params.iter().map(|(n, _)| n.clone()).collect();
        let method =
            crate::typer::type_apply::instantiate_poly(self.u, result, (binder, &names), &types, Some(apply));
        let Type::Method { result: applied, .. } = *self.u.types().get(method) else { return Err(missing) };
        let targs: Vec<TNodeId> = types
            .iter()
            .map(|&t| self.tpd.node(T::TypeTree { inferred: false }, at, Some(t), None))
            .collect();
        let targs = self.tpd.list(&targs);
        let type_apply = self.tpd.node(T::TypeApply { fun: select, args: targs }, at, Some(method), None);
        let args = self.tpd.list(elems);
        Ok(self.tpd.node(T::Apply { fun: type_apply, args }, span.to_synthetic(), Some(applied), None))
    }

    /// `TupleXXL.apply(elems.map(_.asInstanceOf[Object])*)`, cast to the `*:` chain of the
    /// types of `elems`.
    fn tuple_xxl(&mut self, elems: &[TNodeId], span: Span) -> Result<TNodeId, Unsupported> {
        let missing = (span, "TupleXXL");
        let runtime = self.u.package("scala.runtime").ok_or(missing)?;
        let decls = self.u.decl(runtime, "TupleXXL").map_err(|_| missing)?;
        let module = decls.into_iter().find(|&s| self.u.symbol(s).kind == Kind::Val).ok_or(missing)?;
        let module_class = self.u.module_class(module).ok_or(missing)?;
        let apply =
            self.u.decl(module_class, "apply").ok().and_then(|d| d.first().copied()).ok_or(missing)?;
        let info = self.u.info(apply);
        let Type::Method { result, .. } = *self.u.types().get(info) else { return Err(missing) };
        let (Some(pair), Some(empty)) =
            (self.scala_member("*:", Kind::Class), self.scala_member("EmptyTuple", Kind::Val))
        else {
            return Err(missing);
        };
        let at = span.start_pos();
        let module_ref = crate::defn::type_ref(self.u, module);
        let qualifier = self.singleton(module_ref, at).ok_or(missing)?;
        let apply_ref = self.u.intern_type(Type::Sym(Some(module_ref), apply));
        let name = self.term_name(apply);
        let fun = self.tpd.node(T::Select { qualifier, name }, at, Some(apply_ref), Some(apply));
        let object = self.u.builtins().object;
        let object = crate::defn::type_ref(self.u, object);
        let mut cast = Vec::with_capacity(elems.len());
        for &e in elems {
            cast.push(self.cast(e, object)?);
        }
        let repeated = self.repeated_object(object).ok_or(missing)?;
        let elemtpt = self.tpd.node(T::TypeTree { inferred: false }, span, Some(object), None);
        let list = self.tpd.list(&cast);
        let seq = self.tpd.node(T::SeqLiteral { elems: list, elemtpt }, span, Some(repeated), None);
        let tpt = self.tpd.node(T::TypeTree { inferred: false }, span, Some(repeated), None);
        let typed = self.tpd.node(T::Typed { expr: seq, tpt }, span, Some(repeated), None);
        let args = self.tpd.list(&[typed]);
        let call = self.tpd.node(T::Apply { fun, args }, span.to_synthetic(), Some(result), None);
        let pair = crate::defn::type_ref(self.u, pair);
        let mut chain = self.u.intern_type(Type::Sym(None, empty));
        for &e in elems.iter().rev() {
            let t = self.tpd.tpe(e);
            chain = self.u.intern_type(Type::Applied(pair, vec![t, chain]));
        }
        self.cast(call, chain)
    }

    /// `<repeated>[object]`.
    fn repeated_object(&mut self, object: TypeId) -> Option<TypeId> {
        let class = self.scala_member("<repeated>", Kind::Class)?;
        let class = crate::defn::type_ref(self.u, class);
        Some(self.u.intern_type(Type::Applied(class, vec![object])))
    }
}
