// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/scala/quoted/runtime/impl/QuotesImpl.scala
//! `Term.betaReduce` of `quotes.reflect`: the inlining's beta reduction, run without a typer.

use super::{Expanded, Host, Inlining, Lent, MacroCall, MacroError, Reflect};
use crate::ctx::Ctx;
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::Span;
use syntax::names::FreshNames;
use tasty::{SymbolId, TypeId};

/// The host of a reduction, which searches no implicits and expands no macros.
pub struct NoHost;

impl Host for NoHost {
    fn call_ctx(&self, _: TNodeId) -> Option<Ctx> {
        None
    }

    fn summon(&mut self, _: Lent<'_>, _: TypeId, _: Span, _: Ctx) -> Result<Option<TNodeId>, &'static str> {
        Err("an implicit search in Term.betaReduce")
    }

    fn error(&mut self, _: Span, _: String) {}

    fn has_errors(&self) -> bool {
        false
    }

    fn expand_macro(&mut self, _: Lent<'_>, _: &MacroCall) -> Result<Expanded, MacroError> {
        Err(MacroError::Unsupported("a macro call in Term.betaReduce".into()))
    }
}

impl Reflect<'_> {
    /// `Term.betaReduce(t)`, the reduced trees owned by `owner`: `t` through empty blocks and
    /// inlined trees without bindings reduced, `None` if it is not a closure's application.
    pub fn beta_reduce(&mut self, t: TNodeId, owner: SymbolId) -> Result<Option<TNodeId>, &'static str> {
        match *self.tpd.tree(t) {
            T::Block { stats, expr } if stats.is_empty() => return self.beta_reduce(expr, owner),
            T::Inlined { bindings, expansion, .. } if bindings.is_empty() => {
                return self.beta_reduce(expansion, owner);
            }
            _ => {}
        }
        let mut fresh = FreshNames::default();
        let mut host = NoHost;
        let mut i =
            Inlining::new((&mut *self.u, &mut *self.tpd), (&mut *self.names, &mut fresh), &mut host, false);
        let reduced = i.beta_reduce(t, owner).map_err(|(_, what)| what)?;
        Ok((reduced != t).then_some(reduced))
    }
}
