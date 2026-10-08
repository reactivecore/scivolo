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
        let other_typed = |t: &Self, a| matches!(t.ast.tree(a), Tree::Typed { .. }) && !t.is_seq_wildcard(a);
        if arg_ids.iter().any(|&a| matches!(self.ast.tree(a), Tree::NamedArg { .. }) || other_typed(self, a))
        {
            return Err("named or typed pattern");
        }
        if !matches!(self.ast.tree(fun), Tree::Ident { .. } | Tree::Select { .. } | Tree::TypedSplice { .. })
        {
            return Err("extractor with type arguments");
        }
        let (unapp, seq) = self.unapply_fun(fun, ctx)?;
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
        if !self.is_checkable_class_type(own) {
            return Err("extractor of a type that needs a type test");
        }
        let result = self.zonk(result);
        let patterns: Vec<TNodeId> = if seq {
            let get = self.get_match(result)?.ok_or("sequence extractor result type")?;
            let elem = self.seq_elem_type(get).ok_or("sequence extractor result type")?;
            if matches!(self.u.types().get(elem), Type::Bounds(..)) {
                return Err("sequence extractor of a wildcard element type");
            }
            self.typed_seq_patterns(&arg_ids, elem, ctx)?
        } else {
            let arg_types = self.unapply_arg_types(result, arg_ids.len())?;
            arg_ids.iter().zip(arg_types).map(|(&a, t)| self.typed(a, Pt::Type(t), ctx)).collect()
        };
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
        if seq && (self.extension_visible("unapply", ctx) || self.unit_defines_extension("unapply")) {
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
        Ok((unapp, seq))
    }

    /// Whether a package object of the unit defines an extension method named `name`.
    fn unit_defines_extension(&mut self, name: &str) -> bool {
        let objects: Vec<_> =
            self.syms.flags.keys().copied().filter(|&s| self.is_package_object(s)).collect();
        objects.into_iter().any(|c| {
            let members = self.class_members(c, name);
            members.iter().any(|&s| self.u.symbol(s).flags.has(tasty::tags::EXTENSION))
        })
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
}
