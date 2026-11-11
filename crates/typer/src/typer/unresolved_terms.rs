// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Typer.scala
//! Identifiers of terms no binding defines that dotty's `typedIdent` still types: `$scope` of
//! XML literals as `scala.xml.TopScope`; a context bound companion (`experimental.modularity`),
//! refused.

use super::Typer;
use crate::ctx::Ctx;
use crate::tpd::TNodeId;
use syntax::ast::{NodeId, TName};
use syntax::config::Features;
use tasty::Kind;

impl Typer<'_> {
    /// The tree of identifier `id` of term `name`, which no binding of `ctx` defines, if it is not
    /// an error.
    pub fn unresolved_term(&mut self, id: NodeId, name: TName, ctx: Ctx) -> Option<TNodeId> {
        if name.is_type {
            return None;
        }
        let text = self.text(name.name).to_owned();
        if text == names::nme::SCOPE {
            let Some(top) = self.resolve_full_name("scala.xml.TopScope", false) else {
                return Some(self.unsupported(id, "unresolved name $scope"));
            };
            let span = self.ast.span(id);
            return Some(self.build().static_ident(top, span));
        }
        if !self.feature_enabled(Features::MODULARITY, "experimental.modularity", ctx) {
            return None;
        }
        let found = self.find_ref(&text, true, ctx).ok().flatten()?;
        let abstract_type =
            found.syms.iter().any(|&s| matches!(self.u.symbol(s).kind, Kind::Type | Kind::TypeParam));
        abstract_type.then(|| self.unsupported(id, "context bound companion"))
    }
}
