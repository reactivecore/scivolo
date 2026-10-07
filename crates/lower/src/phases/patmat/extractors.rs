//! Plans of extractor patterns (dotty's `unapplyPlan`, `matchArgsPlan`): the case accessors of a
//! Scala 2 case class, a Boolean `unapply`, the `_1`… of a product `unapply`, or the `get` of an
//! option-like one, each component then matched against its argument pattern.

use super::Translator;
use super::plan::{Plan, PlanId, Test};
use crate::phases::case_members::case_accessors;
use crate::symbols::{companion_class, has};
use crate::{Result, unsupported};
use syntax::ast::Span;
use tasty::tags::SYNTHETIC;
use tasty::{SymbolId, Type};
use typer::tpd::{List, TNodeId, Tree};

impl Translator<'_, '_> {
    /// The plan of extractor pattern `pat` with `unapply` method tree `fun` and argument
    /// patterns `args` on `scrutinee`: a null check unless the scrutinee cannot be null.
    pub fn unapply_pattern_plan(
        &mut self,
        scrutinee: SymbolId,
        pat: TNodeId,
        (fun, implicits, args): (TNodeId, List, List),
        on_success: PlanId,
    ) -> Result<PlanId> {
        let span = self.l.tpd.span(pat);
        if !implicits.is_empty() {
            return unsupported(span, "extractors with implicit arguments");
        }
        let fun_type = self.l.u.widen(self.l.tpd.tpe(fun));
        let Type::Method { params, result, .. } = self.l.u.types().get(fun_type).clone() else {
            return unsupported(span, "extractors that are not methods");
        };
        let value = self.l.ident(scrutinee, Span::NONE);
        let arg = self.l.ensure_conforms(value, params[0].1);
        let unapp = self.l.apply(fun, &[arg], result, Span::NONE);
        let unapp = self.l.positioned(unapp);
        let args = self.l.tpd.items(args).to_vec();
        let plan = self.unapply_plan(scrutinee, unapp, &args, on_success)?;
        let info = self.l.u.info(scrutinee);
        let not_null = matches!(self.l.u.erase(info), tasty::Erased::Prim(_));
        if not_null || self.non_null.contains(&scrutinee) {
            return Ok(plan);
        }
        let scrutinee = self.l.ident(scrutinee, Span::NONE);
        Ok(self.add(Plan::Test { test: Test::NonNull, scrutinee, span, on_success: plan }))
    }

    fn unapply_plan(
        &mut self,
        scrutinee: SymbolId,
        unapp: TNodeId,
        args: &[TNodeId],
        on_success: PlanId,
    ) -> Result<PlanId> {
        let Tree::Apply { fun, .. } = *self.l.tpd.tree(unapp) else { unreachable!("an application") };
        let extractor = match *self.l.tpd.tree(fun) {
            Tree::TypeApply { fun, .. } => self.l.tpd.sym(fun),
            _ => self.l.tpd.sym(fun),
        };
        let extractor = extractor.expect("an extractor's symbol");
        let span = self.l.tpd.span(unapp);
        let owner = self.l.u.symbol(extractor).owner.expect("an extractor's owner");
        let accessors =
            companion_class(self.l.u, owner).map(|c| case_accessors(self.l, c)).unwrap_or_default();
        let unapp_type = self.l.tpd.tpe(unapp);
        if self.l.u.is_scala2(extractor)
            && has(self.l.u, extractor, SYNTHETIC)
            && accessors.len() == args.len()
        {
            let components: Vec<TNodeId> = accessors
                .iter()
                .map(|&a| {
                    let value = self.l.ident(scrutinee, Span::NONE);
                    self.select_member(value, a)
                })
                .collect();
            return self.match_args_plan(&components, args, on_success);
        }
        if self.l.class_of(unapp_type).is_some_and(|c| self.l.u.full_name(c) == "scala.Boolean") {
            return Ok(self.add(Plan::Test { test: Test::Guard, scrutinee: unapp, span, on_success }));
        }
        if self.l.u.symbol(extractor).name == "unapplySeq" {
            return unsupported(span, "sequence extractors");
        }
        let arity = args.len();
        self.let_abstract(unapp, None, |t, result| {
            let result_type = t.l.u.info(result);
            let selectors = t.product_selectors(result_type);
            if arity > 0 && selectors.len() == arity {
                let components: Vec<TNodeId> = selectors
                    .iter()
                    .map(|&s| {
                        let value = t.l.ident(result, Span::NONE);
                        t.select_member(value, s)
                    })
                    .collect();
                return t.match_args_plan(&components, args, on_success);
            }
            let Some(get) = t.member_named(result_type, "get") else {
                return unsupported(span, "extractors without `get`");
            };
            let value = t.l.ident(result, Span::NONE);
            let get = t.select_member(value, get);
            let args_plan = t.let_abstract(get, None, |t, get_result| {
                let components = match arity {
                    1 => vec![t.l.ident(get_result, Span::NONE)],
                    _ => {
                        let get_type = t.l.u.info(get_result);
                        let selectors = t.product_selectors(get_type);
                        let components = selectors.iter().map(|&s| {
                            let value = t.l.ident(get_result, Span::NONE);
                            t.select_member(value, s)
                        });
                        components.collect()
                    }
                };
                t.match_args_plan(&components, args, on_success)
            })?;
            let scrutinee = t.l.ident(result, Span::NONE);
            Ok(t.add(Plan::Test { test: Test::NonEmpty, scrutinee, span, on_success: args_plan }))
        })
    }

    /// Binds `components` to fresh variables in turn, then matches them against `args`.
    fn match_args_plan(
        &mut self,
        components: &[TNodeId],
        args: &[TNodeId],
        on_success: PlanId,
    ) -> Result<PlanId> {
        if components.len() != args.len() {
            return unsupported(Span::NONE, "extractor patterns of another arity");
        }
        self.bind_components(components, Vec::new(), args, on_success)
    }

    fn bind_components(
        &mut self,
        components: &[TNodeId],
        mut syms: Vec<SymbolId>,
        args: &[TNodeId],
        on_success: PlanId,
    ) -> Result<PlanId> {
        let Some((&first, rest)) = components.split_first() else {
            return self.match_args(args, &syms, on_success);
        };
        self.let_abstract(first, None, |t, sym| {
            syms.push(sym);
            t.bind_components(rest, syms, args, on_success)
        })
    }

    /// Matches `syms` against `args`, the later ones planned first.
    fn match_args(&mut self, args: &[TNodeId], syms: &[SymbolId], on_success: PlanId) -> Result<PlanId> {
        let (Some((&arg, args)), Some((&sym, syms))) = (args.split_first(), syms.split_first()) else {
            return Ok(on_success);
        };
        let next = self.match_args(args, syms, on_success)?;
        self.pattern_plan(sym, arg, next)
    }
}
