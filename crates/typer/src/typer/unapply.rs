// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Applications.scala
//! Extractor patterns `C(p1, ..., pn)` (dotty's `typedUnApply`): `C.unapply` applied to the
//! matched value, its result's product selectors or `get` the expected types of the patterns.

use super::implicits::conversions::Goal;
use super::{Pt, Typer};
use crate::ctx::{Ctx, Mode};
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::{List, NodeId, Span, TName, Tree};
use tasty::{Type, TypeId, TypeVars};

impl Typer<'_> {
    pub fn typed_unapply(&mut self, id: NodeId, fun: NodeId, args: List, pt: Pt, ctx: Ctx) -> TNodeId {
        match self.unapply_tree(id, fun, args, pt, ctx) {
            Ok(tree) => {
                let unapply = match *self.tpd.tree(tree) {
                    T::UnApply { fun, .. } => self.tpd.sym(self.tpd.meth_part(fun)),
                    _ => None,
                };
                if self.is_invalid_future_infix_op(id, fun, unapply) {
                    return self.unsupported(id, "alphanumeric infix extractor under -source future");
                }
                tree
            }
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
        let zonked = self.zonk(sel);
        let sel_type = self.strip_named_tuple(zonked);
        let sel = if sel_type == zonked { sel } else { sel_type };
        self.retype_tuple_arg_types(sel_type, &arg_ids, ctx);
        let (applied, targs) = (fun, self.unapply_type_args(fun, sel_type, ctx));
        let fun = targs.map_or(fun, |(qual, _)| qual);
        let with_targs = matches!(self.ast.tree(fun), Tree::TypeApply { .. });
        if !with_targs
            && !self.splices.contains_key(&fun)
            && !matches!(
                self.ast.tree(fun),
                Tree::Ident { .. } | Tree::Select { .. } | Tree::TypedSplice { .. }
            )
        {
            return Err("extractor with type arguments");
        }
        let (unapp, seq) = self.unapply_fun(fun, sel_type, ctx)?;
        if with_targs && !self.is_new_instance(unapp) {
            return Err("extractor with type arguments");
        }
        if with_targs && let crate::tpd::Tree::Select { qualifier, .. } = *self.tpd.tree(unapp) {
            let span = self.tpd.span(qualifier);
            self.tpd.get_mut(qualifier).span = span.to_synthetic();
        }
        if self.tpd.sym(self.tpd.meth_part(unapp)).is_some_and(|s| self.is_transparent_inline(s)) {
            return Err("transparent inline extractor");
        }
        if !seq && arg_ids.iter().any(|&a| self.is_seq_wildcard(a)) {
            return Err("sequence wildcard of an extractor without unapplySeq");
        }
        let vars = self.constraint.count();
        let unapp = match targs {
            Some((_, args)) => self.with_unapply_type_args(applied, unapp, args, ctx)?,
            None => self.with_type_vars(unapp)?,
        };
        let fun_tpe = self.tpd.tpe(unapp);
        let info = self.method_info(fun_tpe).unwrap_or(fun_tpe);
        let Type::Method { binder, params, result, implicit: false, .. } = self.u.types().get(info).clone()
        else {
            return Err("not an extractor");
        };
        let [(ref param, arg_type)] = params[..] else { return Err("not an extractor") };
        let implicit = matches!(self.u.types().get(result), Type::Method { implicit: true, .. });
        if !implicit && matches!(self.u.types().get(result), Type::Method { .. } | Type::Poly { .. }) {
            return Err("extractor with more parameters");
        }
        let own = self.pattern_type((sel, arg_type), vars, unapp, ctx)?;
        let sel = self.zonk(sel);
        let tested = self.stripped(own) != self.stripped(sel);
        let result = self.applied_unapply_result(unapp, result, (binder, param), own);
        let (result, implicits) = self.unapply_implicits(unapp, result, own, ctx)?;
        let result = self.simplified(result);
        let named = arg_ids.iter().any(|&a| self.is_named_pattern(a));
        let patterns: Vec<TNodeId> = if seq {
            if named {
                return Err("named pattern of a sequence extractor");
            }
            let types = self.unapply_seq_arg_types(result, arg_ids.len())?;
            self.typed_seq_patterns(&arg_ids, types, ctx)?
        } else {
            let mut values = None;
            if named {
                let names;
                (names, values) = self.named_pattern_elements(result, &arg_ids)?;
                arg_ids = self.adapt_pattern_args(&arg_ids, &names)?;
            }
            let arg_types = match values {
                Some(values) => values,
                None => self.unapply_arg_types(result, arg_ids.len())?,
            };
            arg_ids.iter().zip(arg_types).map(|(&a, t)| self.typed_pattern_arg(a, t, ctx)).collect()
        };
        let (implicits, patterns) = (self.tpd.list(&implicits), self.tpd.list(&patterns));
        let span = self.ast.span(id);
        let tree = self.tpd.node(T::UnApply { fun: unapp, implicits, patterns }, span, Some(own), None);
        self.record_inline_extractor(unapp, ctx);
        if !tested || self.is_bottom_type(sel) {
            return Ok(tree);
        }
        self.type_tested_unapply(id, tree, own, sel, ctx)
    }

    /// `qual.unapply` for extractor `qual`, typed as an expression, a member or an extension
    /// method, or `qual.unapplySeq` if it has neither, an overloaded one applied to a value of
    /// scrutinee type `sel` (dotty's `trySelectUnapply`); whether it is `unapplySeq`.
    pub fn unapply_fun(
        &mut self,
        qual: NodeId,
        sel: TypeId,
        ctx: Ctx,
    ) -> Result<(TNodeId, bool), &'static str> {
        let unsupported = self.unsupported.clone();
        let ctx = ctx.with_mode(Mode::NONE);
        let ident = matches!(self.ast.tree(qual), Tree::Ident { .. });
        let typed = self.typed(qual, Pt::Any, if ident { ctx.with_mode(Mode::EXTRACTOR) } else { ctx });
        let tpe = self.tpd.tpe(typed);
        let tpe = self.widen_if_unstable(tpe);
        let member = self.members_found(tpe, "unapply", false).is_some();
        let extension = !member && self.extension_visible("unapply", ctx);
        let seq_found = !member
            && (self.members_found(tpe, "unapplySeq", false).is_some()
                || self.extension_visible("unapplySeq", ctx));
        if extension && seq_found {
            return Err("extension method");
        }
        if seq_found && !extension && self.view_may_supply_unapply(typed, ctx) {
            return Err("unapply of an implicit view beside unapplySeq");
        }
        let seq = !extension && seq_found;
        let span = self.ast.span(qual);
        let splice = self.ast.node(Tree::TypeTree);
        self.ast.with_span(splice, span);
        self.splices.insert(splice, typed);
        let name = TName { name: self.name(if seq { "unapplySeq" } else { "unapply" }), is_type: false };
        let select = self.ast.node(Tree::Select { qualifier: splice, name });
        self.ast.with_span(select, Span::synthetic(span.start(), span.end()));
        let pt = self.scrutinee_proto(tpe, self.text(name.name).to_owned(), sel, ctx);
        let unapp = self.typed_select(select, splice, name, pt, ctx);
        if self.unsupported != unsupported {
            return Err("extractor outside the subset");
        }
        let chosen = self.tpd.sym(self.tpd.meth_part(unapp)).map(|s| self.u.symbol(s).name.as_str());
        if chosen != Some(self.text(name.name)) {
            return Err("not an extractor");
        }
        Ok((unapp, seq))
    }

    /// Whether an implicit conversion or an extension method of an implicit value may give `qual`
    /// a member `unapply`.
    fn view_may_supply_unapply(&mut self, qual: TNodeId, ctx: Ctx) -> bool {
        let reported = self.diags.items.len();
        let (found, clean, _) = self.speculatively(|t| t.viewed(qual, Goal::Member("unapply"), ctx));
        self.diags.items.truncate(reported);
        match found {
            Ok(None) if clean => false,
            Ok(Some(_)) => true,
            _ => {
                let site = self.u.widen(self.tpd.tpe(qual));
                let q = self.u.class_symbol(site);
                q.is_none_or(|q| self.may_view_to((q, site), "unapply", ctx) != Some(false))
            }
        }
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

    /// Whether `t` has a wildcard type argument, also inside another argument.
    fn has_wildcard(&mut self, t: TypeId) -> bool {
        let mut found = false;
        self.u.map_type(t, &mut |u, part| {
            found |= matches!(u.types().get(part), Type::Bounds(..));
            None
        });
        found
    }

    /// The type of a pattern whose extractor takes `arg_type` against a value of type `sel`:
    /// `sel` if it conforms, else `arg_type` constrained by `sel`, unless only variant type
    /// arguments of an open class would be, and maximized with the binders added to extractor
    /// `unapp`; type variables from id `vars` on instantiated.
    fn pattern_type(
        &mut self,
        (sel, arg_type): (TypeId, TypeId),
        vars: usize,
        unapp: TNodeId,
        ctx: Ctx,
    ) -> Result<TypeId, &'static str> {
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
        if !unconstrained && !self.constrain_pattern(arg_type, sel)? {
            self.constraint.rollback(mark);
            if self.has_abstract_arg(sel) {
                return Err("pattern constraining type parameters (GADT)");
            }
        }
        let bound = self.maximize_type(arg_type, vars, unapp, ctx)?;
        let tpe = self.zonk(arg_type);
        if !bound.is_empty() && self.has_wildcard(tpe) {
            return Err("pattern-bound type against a wildcard argument");
        }
        self.add_binders(unapp, &bound);
        Ok(self.zonk(arg_type))
    }
}
