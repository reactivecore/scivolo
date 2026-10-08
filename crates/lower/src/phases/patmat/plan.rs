//! The plans of matches: decision graphs of tests, variable bindings, labeled blocks, returns from
//! them, sequences and results, built from a match's cases.

use super::Translator;
use crate::{Result, unsupported};
use syntax::ast::Span;
use tasty::SymbolId;
use typer::tpd::{TNodeId, Tree};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct PlanId(pub usize);

pub enum Plan {
    /// `if scrutinee <test> then on_success`, `span` the tested pattern's.
    Test {
        test: Test,
        scrutinee: TNodeId,
        span: Span,
        on_success: PlanId,
    },
    Let {
        sym: SymbolId,
        body: PlanId,
    },
    Labeled {
        sym: SymbolId,
        expr: PlanId,
    },
    Return {
        label: SymbolId,
    },
    Seq {
        head: PlanId,
        tail: PlanId,
    },
    Result {
        tree: TNodeId,
    },
}

pub fn set_on_success(plan: &mut Plan, new: PlanId) {
    if let Plan::Test { on_success, .. } = plan {
        *on_success = new;
    }
}

#[derive(Clone, Copy)]
pub enum Test {
    /// `scrutinee == tree`.
    Equal(TNodeId),
    /// The scrutinee is the condition.
    Guard,
    /// `scrutinee.$isInstanceOf[tpt]`.
    Type(TNodeId),
    /// `scrutinee ne null`.
    NonNull,
    /// `!scrutinee.isEmpty`.
    NonEmpty,
    /// `scrutinee.lengthCompare(len) == 0`, or `>= 0` unless `exact`.
    Length { len: i32, exact: bool },
}

impl Translator<'_, '_> {
    pub fn plan(&self, p: PlanId) -> &Plan {
        &self.plans[p.0]
    }

    pub fn plan_mut(&mut self, p: PlanId) -> &mut Plan {
        &mut self.plans[p.0]
    }

    /// Whether control may leave `p` other than by a return.
    pub fn can_fall_through(&self, p: PlanId) -> bool {
        match *self.plan(p) {
            Plan::Return { .. } | Plan::Result { .. } => false,
            Plan::Test { .. } | Plan::Labeled { .. } => true,
            Plan::Let { body, .. } => self.can_fall_through(body),
            Plan::Seq { tail, .. } => self.can_fall_through(tail),
        }
    }

    /// The plan of match `m`: its cases in turn, a `MatchError` if none matches.
    pub fn match_plan(&mut self, m: TNodeId) -> Result<PlanId> {
        let Tree::Match { selector, cases } = *self.l.tpd.tree(m) else { unreachable!("a match") };
        let cases = self.l.tpd.items(cases).to_vec();
        self.let_abstract(selector, None, |t, scrutinee| {
            let error = t.match_error(scrutinee);
            let mut next = t.add(Plan::Result { tree: error });
            for &c in cases.iter().rev() {
                let head = t.case_def_plan(scrutinee, c)?;
                next = t.add(Plan::Seq { head, tail: next });
            }
            Ok(next)
        })
    }

    /// `throw new MatchError(scrutinee)`.
    fn match_error(&mut self, scrutinee: SymbolId) -> TNodeId {
        let p = self.l.defs.patmat;
        let new = self.l.new_(p.match_error, Span::NONE);
        let arg = self.l.ident(scrutinee, Span::NONE);
        let error = self.l.call(new, p.match_error_ctor, &[arg]);
        let tpe = self.l.tpd.tpe(new);
        self.l.tpd.get_mut(error).tpe = Some(tpe);
        let throw = self.l.ident(p.throw, Span::NONE);
        let nothing = self.nothing_type();
        let apply = self.l.apply(throw, &[error], nothing, Span::NONE);
        self.l.positioned(apply)
    }

    fn case_def_plan(&mut self, scrutinee: SymbolId, c: TNodeId) -> Result<PlanId> {
        let Tree::CaseDef { pat, guard, body } = *self.l.tpd.tree(c) else { unreachable!("a case") };
        let mut on_success = self.add(Plan::Result { tree: body });
        if let Some(guard) = guard {
            let span = self.l.tpd.span(guard);
            on_success = self.add(Plan::Test { test: Test::Guard, scrutinee: guard, span, on_success });
        }
        self.pattern_plan(scrutinee, pat, on_success)
    }

    /// The plan matching the value of `scrutinee` against `pat`, then `on_success`.
    pub fn pattern_plan(&mut self, scrutinee: SymbolId, pat: TNodeId, on_success: PlanId) -> Result<PlanId> {
        let span = self.l.tpd.span(pat);
        let pat = self.swap_bind(pat);
        match *self.l.tpd.tree(pat) {
            Tree::Typed { expr: inner, tpt } => {
                let target = self.l.tpd.tpe(tpt);
                if self.is_singleton(target) {
                    return unsupported(span, "type patterns of singleton types");
                }
                let value = self.l.ident(scrutinee, Span::NONE);
                let cast = self.cast(value, target);
                let body = self.let_abstract(cast, None, |t, casted| {
                    t.non_null.insert(casted);
                    t.pattern_plan(casted, inner, on_success)
                })?;
                let scrutinee = self.l.ident(scrutinee, Span::NONE);
                Ok(self.add(Plan::Test { test: Test::Type(tpt), scrutinee, span, on_success: body }))
            }
            Tree::UnApply { fun, implicits, patterns } => {
                self.unapply_pattern_plan(scrutinee, pat, (fun, implicits, patterns), on_success)
            }
            Tree::SeqLiteral { .. } => unsupported(span, "sequence patterns"),
            Tree::Bind { name, body } if self.l.names.text(name.name) == "_" => {
                self.pattern_plan(scrutinee, body, on_success)
            }
            Tree::Bind { body, .. } => {
                let bound = self.l.tpd.sym(pat).expect("a bound symbol");
                let value = self.l.ident(scrutinee, Span::NONE);
                self.initializer.insert(bound, value);
                self.spans.insert(bound, span);
                let on_success = self.add(Plan::Let { sym: bound, body: on_success });
                self.pattern_plan(scrutinee, body, on_success)
            }
            Tree::Alternative { trees } => {
                let alts = self.l.tpd.items(trees).to_vec();
                self.alts_labeled(|t, onf| {
                    let inner = t.alts_labeled(|t, ons| {
                        let mut next = onf;
                        for &alt in alts.iter().rev() {
                            let head = t.pattern_plan(scrutinee, alt, ons)?;
                            next = t.add(Plan::Seq { head, tail: next });
                        }
                        Ok(next)
                    })?;
                    Ok(t.add(Plan::Seq { head: inner, tail: on_success }))
                })
            }
            _ if self.is_wildcard(pat) || matches!(self.l.tpd.tree(pat), Tree::This { .. }) => Ok(on_success),
            _ => {
                let scrutinee = self.l.ident(scrutinee, Span::NONE);
                Ok(self.add(Plan::Test { test: Test::Equal(pat), scrutinee, span, on_success }))
            }
        }
    }

    /// Whether pattern `pat` matches anything without binding a value (dotty's
    /// `WildcardPattern`).
    fn is_wildcard(&self, pat: TNodeId) -> bool {
        match *self.l.tpd.tree(pat) {
            Tree::Ident { name } => matches!(self.l.names.text(name.name), "_" | "_*"),
            Tree::Bind { name, body } => self.l.names.text(name.name) == "_" && self.is_wildcard(body),
            Tree::Alternative { trees } => self.l.tpd.items(trees).iter().all(|&p| self.is_wildcard(p)),
            _ => false,
        }
    }
}
