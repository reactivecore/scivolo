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
        let arg_ids = self.ast.items(args).to_vec();
        if arg_ids.iter().any(|&a| matches!(self.ast.tree(a), Tree::NamedArg { .. } | Tree::Typed { .. })) {
            return Err("named or sequence pattern");
        }
        if !matches!(self.ast.tree(fun), Tree::Ident { .. } | Tree::Select { .. } | Tree::TypedSplice { .. })
        {
            return Err("extractor with type arguments");
        }
        let unapp = self.unapply_fun(fun, ctx)?;
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
        if !self.is_checkable_class_type(own) {
            return Err("extractor of a type that needs a type test");
        }
        let result = self.zonk(result);
        let arg_types = self.unapply_arg_types(result, arg_ids.len())?;
        let patterns: Vec<TNodeId> =
            arg_ids.iter().zip(arg_types).map(|(&a, t)| self.typed(a, Pt::Type(t), ctx)).collect();
        let (implicits, patterns) = (self.tpd.list(&[]), self.tpd.list(&patterns));
        let span = self.ast.span(id);
        let tree = self.tpd.node(T::UnApply { fun: unapp, implicits, patterns }, span, Some(own), None);
        let sel = self.zonk(sel);
        if own == sel || sel == tasty::NOTHING {
            return Ok(tree);
        }
        let tpt = self.type_tree(own, span.end_pos(), false);
        Ok(self.tpd.node(T::Typed { expr: tree, tpt }, span.to_synthetic(), Some(own), None))
    }

    /// `qual.unapply` for extractor `qual`, typed as an expression.
    fn unapply_fun(&mut self, qual: NodeId, ctx: Ctx) -> Result<TNodeId, &'static str> {
        let name = TName { name: self.name("unapply"), is_type: false };
        let select = self.ast.node(Tree::Select { qualifier: qual, name });
        let span = self.ast.span(qual);
        self.ast.with_span(select, Span::synthetic(span.start(), span.end()));
        let unsupported = self.unsupported.clone();
        let unapp = self.typed_select(select, qual, name, Pt::Any, ctx.with_mode(Mode::NONE));
        if self.unsupported != unsupported {
            return Err("extractor outside the subset");
        }
        Ok(unapp)
    }

    /// The type of a pattern whose extractor takes `arg_type` against a value of type `sel`:
    /// `sel` if it conforms, else `arg_type` constrained by `sel` and maximized; type variables
    /// from id `vars` on instantiated.
    fn pattern_type(&mut self, sel: TypeId, arg_type: TypeId, vars: usize) -> Result<TypeId, &'static str> {
        let mark = self.constraint.mark();
        if self.conforms(sel, arg_type) {
            self.fully_defined_as(arg_type, true)?;
            return Ok(if sel == tasty::NOTHING { self.zonk(arg_type) } else { sel });
        }
        self.constraint.rollback(mark);
        let open = self.u.class_symbol(arg_type).is_none_or(|c| {
            let flags = self.u.symbol(c).flags;
            !flags.has(tasty::tags::FINAL) && !flags.has(tasty::tags::CASE)
        });
        let variances = self.variances(arg_type, vars);
        if open && !variances.is_empty() {
            return Err("type arguments of a pattern of an open class");
        }
        if !self.conforms(arg_type, sel) {
            self.constraint.rollback(mark);
        }
        let arg_type = self.zonk(arg_type);
        let variances = self.variances(arg_type, vars);
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

    /// The expected types of `n` patterns of an extractor with result type `result` (dotty's
    /// `UnapplyArgs.argTypes` for `unapply`).
    fn unapply_arg_types(&mut self, result: TypeId, n: usize) -> Result<Vec<TypeId>, &'static str> {
        let types = if n > 0
            && self.derives_from_named(result, "scala.Product")
            && self.product_selectors(result)?.len() == n
        {
            self.product_selectors(result)?
        } else if let Some(get) = self.get_match(result)? {
            let sels = if n > 1 && !self.derives_from_named(get, "scala.collection.Seq") {
                self.product_selectors(get)?
            } else {
                Vec::new()
            };
            if sels.len() == n { sels } else { vec![get] }
        } else if self.derives_from_named(result, "scala.Boolean") {
            Vec::new()
        } else {
            return Err("extractor result type");
        };
        if types.len() != n {
            return Err("wrong number of patterns");
        }
        Ok(types)
    }

    /// The types of the product selectors `_1`, `_2`, ... of `t`.
    fn product_selectors(&mut self, t: TypeId) -> Result<Vec<TypeId>, &'static str> {
        let mut out = Vec::new();
        while let Some(s) = self.extractor_member(t, &format!("_{}", out.len() + 1))? {
            out.push(s);
        }
        Ok(out)
    }

    /// The type of `get` if `t` has it and a `Boolean` member `isEmpty`.
    fn get_match(&mut self, t: TypeId) -> Result<Option<TypeId>, &'static str> {
        let empty = self.extractor_member(t, "isEmpty")?;
        if !empty.is_some_and(|e| self.derives_from_named(e, "scala.Boolean")) {
            return Ok(None);
        }
        self.extractor_member(t, "get")
    }

    /// The type of parameterless member `name` of `t` (dotty's `extractorMemberType`).
    fn extractor_member(&mut self, t: TypeId, name: &str) -> Result<Option<TypeId>, &'static str> {
        let Some(found) = self.members_found(t, name, false) else { return Ok(None) };
        let [sym] = found.syms[..] else { return Err("overloaded extractor member") };
        let tpe = self.ref_type(Some(t), sym);
        let info = self.method_info(tpe).unwrap_or(tpe);
        Ok(match *self.u.types().get(info) {
            Type::Method { .. } | Type::Poly { .. } => None,
            Type::ByName(r) => Some(r),
            _ => Some(info),
        })
    }

    fn derives_from_named(&mut self, t: TypeId, class: &str) -> bool {
        let base = self.resolve_full_name(class, true);
        let t = self.u.widen(t);
        match (self.u.class_symbol(t), base) {
            (Some(c), Some(b)) => self.u.derives_from(c, b),
            _ => false,
        }
    }
}
