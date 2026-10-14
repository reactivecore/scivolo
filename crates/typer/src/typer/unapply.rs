//! Extractor patterns `C(p1, ..., pn)` (dotty's `typedUnApply`): `C.unapply` applied to the
//! matched value, its result's product selectors or `get` the expected types of the patterns.

use super::{Pt, Typer};
use crate::ctx::{Ctx, Mode};
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::{List, NodeId, Span, TName, Tree};
use tasty::{Type, TypeId, TypeVars, VarId};

impl Typer<'_> {
    pub fn typed_unapply(&mut self, id: NodeId, fun: NodeId, args: List, pt: Pt, ctx: Ctx) -> TNodeId {
        match self.unapply_tree(id, fun, args, pt, ctx) {
            Ok(tree) => tree,
            Err(what) => self.unsupported(id, what),
        }
    }

    fn unapply_tree(
        &mut self,
        id: NodeId,
        fun: NodeId,
        args: List,
        pt: Pt,
        ctx: Ctx,
    ) -> Result<TNodeId, &'static str> {
        let Pt::Type(sel) = pt else { return Err("extractor without expected type") };
        let mut arg_ids = self.ast.items(args).to_vec();
        let sel_type = self.zonk(sel);
        self.retype_tuple_arg_types(sel_type, &arg_ids, ctx);
        if !matches!(self.ast.tree(fun), Tree::Ident { .. } | Tree::Select { .. } | Tree::TypedSplice { .. })
        {
            return Err("extractor with type arguments");
        }
        let (unapp, seq) = self.unapply_fun(fun, ctx)?;
        if self.tpd.sym(self.tpd.meth_part(unapp)).is_some_and(|s| self.is_transparent_inline(s)) {
            return Err("transparent inline extractor");
        }
        if !seq && arg_ids.iter().any(|&a| self.is_seq_wildcard(a)) {
            return Err("sequence wildcard of an extractor without unapplySeq");
        }
        let vars = self.constraint.count();
        let unapp = self.with_type_vars(unapp)?;
        let fun_tpe = self.tpd.tpe(unapp);
        let info = self.method_info(fun_tpe).unwrap_or(fun_tpe);
        let Type::Method { params, result, implicit: false, .. } = self.u.types().get(info).clone() else {
            return Err("not an extractor");
        };
        let [(_, arg_type)] = params[..] else { return Err("not an extractor") };
        if matches!(self.u.types().get(result), Type::Method { .. } | Type::Poly { .. }) {
            return Err("extractor with more parameters");
        }
        let own = self.pattern_type(sel, arg_type, vars)?;
        let sel_now = self.zonk(sel);
        if self.stripped(own) != self.stripped(sel_now) && !self.is_checkable_class_type(own) {
            return Err("extractor of a type that needs a type test");
        }
        let result = self.zonk(result);
        let named = arg_ids.iter().any(|&a| self.is_named_pattern(a));
        let patterns: Vec<TNodeId> = if seq {
            if named {
                return Err("named pattern of a sequence extractor");
            }
            let types = self.unapply_seq_arg_types(result, arg_ids.len())?;
            self.typed_seq_patterns(&arg_ids, types, ctx)?
        } else {
            if named {
                let target = self.named_patterns_type(result, arg_ids.len())?;
                arg_ids = self.adapt_pattern_args(&arg_ids, target)?;
            }
            let arg_types = self.unapply_arg_types(result, arg_ids.len())?;
            arg_ids.iter().zip(arg_types).map(|(&a, t)| self.typed_pattern_arg(a, t, ctx)).collect()
        };
        let (implicits, patterns) = (self.tpd.list(&[]), self.tpd.list(&patterns));
        let span = self.ast.span(id);
        let tree = self.tpd.node(T::UnApply { fun: unapp, implicits, patterns }, span, Some(own), None);
        let sel = self.zonk(sel);
        if self.stripped(own) == self.stripped(sel) || self.is_bottom_type(sel) {
            return Ok(tree);
        }
        let tpt = self.type_tree(own, span.end_pos(), false);
        Ok(self.tpd.node(T::Typed { expr: tree, tpt }, span.to_synthetic(), Some(own), None))
    }

    /// `qual.unapply` for extractor `qual`, typed as an expression, or `qual.unapplySeq` if it
    /// has no `unapply`; whether it is `unapplySeq`.
    fn unapply_fun(&mut self, qual: NodeId, ctx: Ctx) -> Result<(TNodeId, bool), &'static str> {
        let unsupported = self.unsupported.clone();
        let ctx = ctx.with_mode(Mode::NONE);
        let typed = self.typed(qual, Pt::Any, ctx);
        let tpe = self.tpd.tpe(typed);
        let tpe = self.widen_if_unstable(tpe);
        let seq = self.members_found(tpe, "unapply", false).is_none()
            && self.members_found(tpe, "unapplySeq", false).is_some();
        if seq && self.extension_visible("unapply", ctx) {
            return Err("extension method");
        }
        let span = self.ast.span(qual);
        let splice = self.ast.node(Tree::TypeTree);
        self.ast.with_span(splice, span);
        self.splices.insert(splice, typed);
        let name = TName { name: self.name(if seq { "unapplySeq" } else { "unapply" }), is_type: false };
        let select = self.ast.node(Tree::Select { qualifier: splice, name });
        self.ast.with_span(select, Span::synthetic(span.start(), span.end()));
        let unapp = self.typed_select(select, splice, name, Pt::Any, ctx);
        if self.unsupported != unsupported {
            return Err("extractor outside the subset");
        }
        if !matches!(self.tpd.tree(unapp), crate::tpd::Tree::Select { .. }) {
            return Err("extension method as an extractor");
        }
        Ok((unapp, seq))
    }

    /// Whether a type argument of `t`, or a part of one, is a type parameter or an abstract type.
    fn has_abstract_arg(&mut self, t: TypeId) -> bool {
        let widened = self.u.widen(t);
        let Type::Applied(_, args) = self.u.types().get(widened).clone() else { return false };
        let mut found = false;
        for a in args {
            self.u.map_type(a, &mut |u, part| {
                if let Type::Sym(_, s) = *u.types().get(part) {
                    found |= match u.symbol(s).kind {
                        tasty::Kind::TypeParam => true,
                        tasty::Kind::Type => {
                            let info = u.info(s);
                            !matches!(u.types().get(info), Type::Alias(_))
                        }
                        _ => false,
                    };
                }
                None
            });
        }
        found
    }

    /// The type of a pattern whose extractor takes `arg_type` against a value of type `sel`:
    /// `sel` if it conforms, else `arg_type` constrained by `sel`, unless only variant type
    /// arguments of an open class would be, and maximized; type variables from id `vars` on
    /// instantiated.
    fn pattern_type(&mut self, sel: TypeId, arg_type: TypeId, vars: usize) -> Result<TypeId, &'static str> {
        let mark = self.constraint.mark();
        if self.conforms(sel, arg_type) {
            self.fully_defined_as(arg_type, true)?;
            return Ok(if self.is_bottom_type(sel) {
                self.zonk(arg_type)
            } else {
                self.without_unchecked(sel)
            });
        }
        self.constraint.rollback(mark);
        let open = self.u.class_symbol(arg_type).is_none_or(|c| {
            let flags = self.u.symbol(c).flags;
            !flags.has(tasty::tags::FINAL) && !flags.has(tasty::tags::CASE)
        });
        let variances = self.variances(arg_type, None, vars);
        let unconstrained = open && !variances.is_empty();
        if unconstrained && variances.values().any(|&v| v == 0) {
            return Err("invariant type arguments of a pattern of an open class");
        }
        if !unconstrained && !self.conforms(arg_type, sel) {
            self.constraint.rollback(mark);
            if self.has_abstract_arg(sel) {
                return Err("pattern constraining type parameters (GADT)");
            }
        }
        let arg_type = self.zonk(arg_type);
        let variances = self.variances(arg_type, None, vars);
        for i in vars..self.constraint.count() {
            let v = VarId(i as u32);
            if self.constraint.instance(v).is_some() {
                continue;
            }
            let from_below = match variances.get(&v.0) {
                None => continue,
                Some(1) => false,
                Some(-1) => true,
                Some(_) => {
                    let (lo, hi) = self.constraint.bounds(v);
                    if lo != hi {
                        return Err("pattern-bound type variable");
                    }
                    false
                }
            };
            self.instantiate_var(v, from_below)?;
        }
        Ok(self.zonk(arg_type))
    }
}
