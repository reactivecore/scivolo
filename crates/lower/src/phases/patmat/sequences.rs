//! Plans of sequence patterns (dotty's `unapplySeqPlan`, `matchElemsPlan`,
//! `unapplyProductSeqPlan`): a length test, the elements selected by `apply(i)` matched against
//! the element patterns, a vararg pattern matched against `drop(n)`, or `toSeq` if it comes first;
//! the product selectors before a sequence matched first.

use super::Translator;
use super::plan::{Plan, PlanId, Test};
use crate::{Result, unsupported};
use syntax::ast::Span;
use tasty::{Kind, SymbolId, Type, TypeId};
use typer::tpd::{TNodeId, Tree};

impl Translator<'_, '_> {
    /// Whether `tpe` has the members a sequence match selects (dotty's `unapplySeqTypeElemTp`).
    pub fn is_sequence(&mut self, tpe: TypeId) -> bool {
        ["apply", "drop", "toSeq"].iter().all(|m| self.method_named(tpe, m).is_some())
            && ["lengthCompare", "length"].iter().any(|m| self.method_named(tpe, m).is_some())
    }

    /// The number of product selectors of `tpe` if they are at most `n + 1` and the last is a
    /// sequence (dotty's `isProductSeqMatch`).
    pub fn product_seq_arity(&mut self, tpe: TypeId, n: usize) -> Option<usize> {
        let selectors = self.product_selectors(tpe, true);
        let info = self.l.u.member_info(tpe, *selectors.last()?);
        let last = match self.l.u.types().get(info).clone() {
            Type::Method { params, result, .. } if params.is_empty() => result,
            Type::ByName(result) => result,
            _ => info,
        };
        (selectors.len() <= n + 1 && self.is_sequence(last)).then_some(selectors.len())
    }

    /// The plan matching the first `arity - 1` product selectors of `result` against as many of
    /// `args`, then its last, a sequence, against the others (dotty's `unapplyProductSeqPlan`).
    pub fn unapply_product_seq_plan(
        &mut self,
        result: SymbolId,
        args: &[TNodeId],
        arity: usize,
        on_success: PlanId,
    ) -> Result<PlanId> {
        let tpe = self.l.u.info(result);
        let mut selectors: Vec<TNodeId> = (self.product_selectors(tpe, false).into_iter())
            .map(|s| {
                let value = self.l.ident(result, Span::NONE);
                self.select_member(value, s)
            })
            .collect();
        let last = selectors.pop().expect("a sequence selector");
        let (leading, rest) = args.split_at(arity - 1);
        let match_seq = self.let_abstract(last, None, |t, seq| t.unapply_seq_plan(seq, rest, on_success))?;
        self.match_args_plan(&selectors[..arity - 1], leading, match_seq)
    }

    /// The plan matching sequence `seq` against element patterns `args`, then `on_success`.
    pub fn unapply_seq_plan(
        &mut self,
        seq: SymbolId,
        args: &[TNodeId],
        on_success: PlanId,
    ) -> Result<PlanId> {
        let Some(i) = args.iter().position(|&a| self.var_arg(a).is_some()) else {
            let len = args.len() as i32;
            return self.match_elems_plan(seq, args, Test::Length { len, exact: true }, on_success);
        };
        let (leading, rest) = args.split_at(i);
        let arg = self.var_arg(rest[0]).expect("a vararg pattern");
        if rest.len() > 1 {
            return unsupported(self.l.tpd.span(rest[1]), "element patterns after a vararg pattern");
        }
        let value = self.l.ident(seq, Span::NONE);
        let remaining = if leading.is_empty() {
            let to_seq = self.method_of(seq, "toSeq");
            self.call_member(value, to_seq, &[])
        } else {
            let drop = self.method_of(seq, "drop");
            let n = self.l.int_literal(leading.len() as i32);
            self.call_member(value, drop, &[n])
        };
        let match_remaining =
            self.let_abstract(remaining, None, |t, rest| t.pattern_plan(rest, arg, on_success))?;
        let len = leading.len() as i32;
        self.match_elems_plan(seq, leading, Test::Length { len, exact: false }, match_remaining)
    }

    /// The plan testing the length of `seq`, then matching `seq(0)`… against `args`.
    fn match_elems_plan(
        &mut self,
        seq: SymbolId,
        args: &[TNodeId],
        length: Test,
        on_success: PlanId,
    ) -> Result<PlanId> {
        let apply = self.method_of(seq, "apply");
        let selectors: Vec<TNodeId> = (0..args.len())
            .map(|i| {
                let value = self.l.ident(seq, Span::NONE);
                let index = self.l.int_literal(i as i32);
                self.call_member(value, apply, &[index])
            })
            .collect();
        let plan = self.match_args_plan(&selectors, args, on_success)?;
        if let Test::Length { len: 0, exact: false } = length {
            return Ok(plan);
        }
        let scrutinee = self.l.ident(seq, Span::NONE);
        let span = self.spans.get(&seq).copied().unwrap_or(Span::NONE);
        Ok(self.add(Plan::Test { test: length, scrutinee, span, on_success: plan }))
    }

    /// The pattern a vararg pattern `pat` matches the remaining elements against (dotty's
    /// `VarArgPattern`).
    fn var_arg(&mut self, pat: TNodeId) -> Option<TNodeId> {
        let pat = self.swap_bind(pat);
        let Tree::Typed { expr, tpt } = *self.l.tpd.tree(pat) else { return None };
        let tpe = self.l.tpd.tpe(tpt);
        let class = self.l.u.class_symbol(tpe)?;
        (self.l.u.full_name(class) == "scala.<repeated>").then_some(expr)
    }

    /// `scrutinee.lengthCompare(len) == 0`, or `>= 0` unless `exact`; without `lengthCompare`,
    /// `scrutinee.length == len` or `>= len`.
    pub fn length_test(&mut self, scrutinee: TNodeId, len: i32, exact: bool) -> TNodeId {
        let tpe = self.l.tpd.tpe(scrutinee);
        let len = self.l.int_literal(len);
        let (compared, operand) = match self.method_named(tpe, "lengthCompare") {
            Some(length_compare) => {
                (self.call_member(scrutinee, length_compare, &[len]), self.l.int_literal(0))
            }
            None => {
                let length = self.method_named(tpe, "length").expect("`length`");
                (self.call_member(scrutinee, length, &[]), len)
            }
        };
        let op = if exact { "==" } else { ">=" };
        let op = self.l.operator(compared, op, operand).expect("an `Int` comparison");
        self.l.call(compared, op, &[operand])
    }

    /// `qual.m(args)`, typed as `m`'s result is seen from `qual`'s type.
    fn call_member(&mut self, qual: TNodeId, m: SymbolId, args: &[TNodeId]) -> TNodeId {
        let prefix = self.l.tpd.tpe(qual);
        let info = self.l.u.member_info(prefix, m);
        let result = self.l.u.result_type(info);
        let fun = self.l.select(qual, m, Span::NONE);
        self.l.tpd.get_mut(fun).tpe = Some(info);
        let fun = self.l.positioned(fun);
        match *self.l.u.types().get(info) {
            Type::Method { .. } | Type::Poly { .. } => {}
            Type::ByName(result) => {
                self.l.tpd.get_mut(fun).tpe = Some(result);
                return fun;
            }
            _ => return fun,
        }
        let apply = self.l.apply(fun, args, result, Span::NONE);
        self.l.positioned(apply)
    }

    /// The method `name` of sequence `seq`, which `is_sequence` found.
    fn method_of(&mut self, seq: SymbolId, name: &str) -> SymbolId {
        let tpe = self.l.u.info(seq);
        self.method_named(tpe, name).expect("a sequence method")
    }

    /// The method named `name` of type `tpe`'s class or its bases taking one `Int`, or nothing
    /// for `toSeq` and `length`.
    fn method_named(&mut self, tpe: TypeId, name: &str) -> Option<SymbolId> {
        let class = self.l.class_of(tpe)?;
        let int = self.l.u.class_ref("scala.Int");
        let int = self.l.u.class_symbol(int);
        for base in self.l.u.linearization(class) {
            for m in self.l.u.decl(base, name).unwrap_or_default() {
                if self.l.u.symbol(m).kind != Kind::Def {
                    continue;
                }
                let info = self.l.u.info(m);
                let (params, result) = match self.l.u.types().get(info) {
                    Type::Method { params, result, .. } => (params.clone(), *result),
                    _ => (Vec::new(), info),
                };
                if matches!(self.l.u.types().get(result), Type::Method { .. }) {
                    continue;
                }
                let fits = match name {
                    "toSeq" | "length" => params.is_empty(),
                    _ => params.len() == 1 && self.l.u.class_symbol(params[0].1) == int,
                };
                if fits {
                    return Some(m);
                }
            }
        }
        None
    }
}
