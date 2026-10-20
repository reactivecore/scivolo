// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Typer.scala
//! Implicit clauses whose missing arguments are left to defaults (the fallback of dotty's
//! `addImplicitArgs` in `adaptNoArgsImplicitMethod`).

use super::super::{Pt, Typer};
use crate::ctx::Ctx;
use crate::tpd::TNodeId;
use syntax::ast::{Attrs, NodeId, TName, Tree};
use tasty::{SymbolId, TypeId};

/// The classes whose instances dotty's `Synthesizer` makes.
const SYNTHESIZED: &[&str] = &[
    "scala.reflect.ClassTag",
    "scala.reflect.TypeTest",
    "scala.CanEqual",
    "scala.ValueOf",
    "scala.util.TupledFunction",
    "scala.deriving.Mirror",
    "scala.deriving.Mirror$.Product",
    "scala.deriving.Mirror$.Sum",
    "scala.reflect.Manifest",
    "scala.reflect.OptManifest",
    "scala.Singleton",
    "scala.Precise",
];

impl Typer<'_> {
    /// Whether parameter `n` of the implicit clause `tree` applies, of type `formal`, has a default
    /// argument that applies when no implicit is found, `formal` a class the compiler does not
    /// synthesize.
    pub fn defaultable(&mut self, tree: TNodeId, meth: Option<SymbolId>, n: usize, formal: TypeId) -> bool {
        let Some(m) = meth else { return false };
        let dealiased = self.u.dealias(formal);
        let class = self.direct_class(dealiased);
        let synthesized = class.is_some_and(|c| SYNTHESIZED.contains(&self.u.full_name(c).as_str()));
        class.is_some() && !synthesized && self.is_fully_defined(formal) && self.may_default(tree, m, n)
    }

    /// `tree` applied to its implicit clause, of parameters `names`, by an application typed anew
    /// with the arguments `found` named and the others left to their defaults.
    pub fn with_default_implicits(
        &mut self,
        tree: TNodeId,
        names: &[String],
        found: &[Option<TNodeId>],
        ctx: Ctx,
    ) -> TNodeId {
        let span = self.tpd.span(tree);
        let mut named: Vec<NodeId> = Vec::new();
        for (n, &arg) in names.iter().zip(found) {
            let Some(arg) = arg else { continue };
            let arg = self.spliced(arg);
            let name = TName { name: self.name(n), is_type: false };
            let node = self.ast.node(Tree::NamedArg { name, arg });
            self.ast.with_span(node, self.ast.span(arg));
            named.push(node);
        }
        let fun = self.spliced(tree);
        let args = self.ast.list(&named);
        let app = self.ast.node(Tree::Apply { fun, args });
        self.ast.with_span(app, span);
        self.ast.add_attrs(app, Attrs::USING_APPLY);
        self.typed(app, Pt::Any, ctx)
    }
}
