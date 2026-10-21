// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/transform/PatternMatcher.scala
//! Plans of extractor patterns (dotty's `unapplyPlan`, `matchArgsPlan`): the case accessors of a
//! Scala 2 case class, a Boolean `unapply`, the `_1`… of a product `unapply`, the elements of a
//! generic tuple (`generic_tuples.rs`), or the `get` of an option-like one (its `_1` for a named
//! pattern of a named tuple of one element), each component then matched against its argument
//! pattern, after the extractor's implicit arguments; the sequence an `unapplySeq` returns
//! (`sequences.rs`).

use super::Translator;
use super::plan::{Plan, PlanId, Test};
use crate::phases::case_members::case_accessors;
use crate::symbols::has;
use crate::{Result, unsupported};
use syntax::ast::Span;
use tasty::tags::SYNTHETIC;
use tasty::{SymbolId, Type};
use typer::tpd::{List, TNodeId};

impl Translator<'_, '_> {
    /// The plan of extractor pattern `pat` with `unapply` method tree `fun` and argument
    /// patterns `args` on `scrutinee`: a null check unless the scrutinee cannot be null; `throw
    /// null` past it for a scrutinee of type `Null` or `Nothing`.
    pub fn unapply_pattern_plan(
        &mut self,
        scrutinee: SymbolId,
        pat: TNodeId,
        (fun, implicits, args): (TNodeId, List, List),
        on_success: PlanId,
    ) -> Result<PlanId> {
        let span = self.l.tpd.span(pat);
        let fun_type = self.l.u.widen(self.l.tpd.tpe(fun));
        let Type::Method { params, result, .. } = self.l.u.types().get(fun_type).clone() else {
            return unsupported(span, "extractors that are not methods");
        };
        let info = self.l.u.info(scrutinee);
        let erased = self.l.u.erase(info);
        let plan = if matches!(erased, tasty::Erased::Null | tasty::Erased::Nothing) {
            let tree = self.throw_null();
            self.add(Plan::Result { tree })
        } else {
            let value = self.l.ident(scrutinee, Span::NONE);
            let arg = self.l.ensure_conforms(value, params[0].1);
            let unapp = self.l.apply(fun, &[arg], result, Span::NONE);
            let mut unapp = self.l.positioned(unapp);
            if !implicits.is_empty() {
                let Type::Method { result, .. } = *self.l.u.types().get(result) else {
                    return unsupported(span, "extractors with implicit arguments");
                };
                let implicits = self.l.tpd.items(implicits).to_vec();
                unapp = self.l.apply(unapp, &implicits, result, Span::NONE);
            }
            let unapp = self.l.positioned(unapp);
            let args = self.l.tpd.items(args).to_vec();
            self.unapply_plan(scrutinee, unapp, &args, on_success)?
        };
        let not_null = matches!(erased, tasty::Erased::Prim(_) | tasty::Erased::Nothing);
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
        let extractor = self.l.tpd.sym(self.l.tpd.meth_part(unapp)).expect("an extractor's symbol");
        let span = self.l.tpd.span(unapp);
        if self.l.u.symbol(extractor).name == "unapplySeq" {
            return self.let_abstract(unapp, None, |t, result| {
                let result_type = t.l.u.info(result);
                if t.is_sequence(result_type) {
                    return t.unapply_seq_plan(result, args, on_success);
                }
                if let Some(arity) = t.product_seq_arity(result_type, args.len())? {
                    return t.unapply_product_seq_plan(result, args, arity, on_success);
                }
                if let Some(plan) = t.generic_tuple_plan(result, args, on_success)? {
                    return Ok(plan);
                }
                let Some(get) = t.member_named(result_type, "get") else {
                    return unsupported(span, "sequence extractors without `get`");
                };
                let value = t.l.ident(result, Span::NONE);
                let get = t.select_member(value, get);
                let args_plan = t.let_abstract(get, None, |t, get_result| {
                    let get_type = t.l.u.info(get_result);
                    if t.is_sequence(get_type) {
                        return t.unapply_seq_plan(get_result, args, on_success);
                    }
                    let Some(arity) = t.product_seq_arity(get_type, args.len())? else {
                        return unsupported(span, "sequence extractors of another result type");
                    };
                    t.unapply_product_seq_plan(get_result, args, arity, on_success)
                })?;
                let scrutinee = t.l.ident(result, Span::NONE);
                Ok(t.add(Plan::Test { test: Test::NonEmpty, scrutinee, span, on_success: args_plan }))
            });
        }
        let owner = self.l.u.symbol(extractor).owner.expect("an extractor's owner");
        let accessors =
            self.l.u.companion_class(owner).map(|c| case_accessors(self.l, c)).unwrap_or_default();
        let unapp_type = self.l.tpd.tpe(unapp);
        let named_field = self.is_named_field_of_get(unapp_type, args);
        if self.l.u.is_scala2(extractor)
            && has(self.l.u, extractor, SYNTHETIC)
            && accessors.len() == args.len()
            && !named_field
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
        let arity = args.len();
        self.let_abstract(unapp, None, |t, result| {
            let result_type = t.l.u.info(result);
            let selectors = t.product_selectors(result_type, true)?;
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
            if let Some(plan) = t.generic_tuple_plan(result, args, on_success)? {
                return Ok(plan);
            }
            let Some(get) = t.member_named(result_type, "get") else {
                return unsupported(span, "extractors without `get`");
            };
            let value = t.l.ident(result, Span::NONE);
            let get = t.select_member(value, get);
            let args_plan = t.let_abstract(get, None, |t, get_result| {
                let components = match arity {
                    1 if !named_field => vec![t.l.ident(get_result, Span::NONE)],
                    _ => {
                        let get_type = t.l.u.info(get_result);
                        let selectors = t.product_selectors(get_type, false)?;
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
    pub fn match_args_plan(
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
