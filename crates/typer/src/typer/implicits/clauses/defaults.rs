// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Typer.scala
//! Implicit clauses whose missing arguments are left to defaults (the fallback of dotty's
//! `addImplicitArgs` in `adaptNoArgsImplicitMethod`).

use super::super::super::{Pt, Typer};
use super::ImplicitClause;
use crate::defn::{
    CAN_EQUAL, CLASS_TAG, MANIFEST, MIRROR, MIRROR_PRODUCT, MIRROR_SUM, OPT_MANIFEST, TUPLED_FUNCTION,
    TYPE_TEST, VALUE_OF,
};
use crate::tpd::TNodeId;
use syntax::ast::{Attrs, NodeId, TName, Tree};
use tasty::TypeId;

/// The classes whose instances dotty's `Synthesizer` makes.
const SYNTHESIZED: &[&str] = &[
    CLASS_TAG,
    TYPE_TEST,
    CAN_EQUAL,
    VALUE_OF,
    TUPLED_FUNCTION,
    MIRROR,
    MIRROR_PRODUCT,
    MIRROR_SUM,
    MANIFEST,
    OPT_MANIFEST,
    "scala.Singleton",
    "scala.Precise",
];

impl ImplicitClause {
    /// Whether parameter `n` of type `formal` may take its default argument when no implicit is
    /// found, as `(using o: Options = Options())` does, unless `formal` is a type the compiler
    /// synthesizes, such as `ClassTag[T]`.
    pub fn defaultable(&self, t: &mut Typer<'_>, n: usize, formal: TypeId) -> bool {
        let Some(m) = self.meth else { return false };
        let dealiased = t.u.dealias(formal);
        let class = t.types().direct_class(dealiased);
        let synthesized = class.is_some_and(|c| SYNTHESIZED.contains(&t.u.full_name(c).as_str()));
        class.is_some()
            && !synthesized
            && t.types().is_fully_defined(formal)
            && t.may_default(self.tree, m, n)
    }

    /// Types the tree applied anew to the clause, the arguments `found` passed by name and the
    /// others left to their defaults, as `f(using a = x)`.
    pub fn with_defaults(&self, t: &mut Typer<'_>, found: &[Option<TNodeId>]) -> TNodeId {
        let span = t.tpd.span(self.tree);
        let mut named: Vec<NodeId> = Vec::new();
        for (n, &arg) in self.names.iter().zip(found) {
            let Some(arg) = arg else { continue };
            let arg = t.spliced(arg);
            let name = TName { name: t.name(n), is_type: false };
            let node = t.ast.node(Tree::NamedArg { name, arg });
            t.ast.with_span(node, t.ast.span(arg));
            named.push(node);
        }
        let fun = t.spliced(self.tree);
        let args = t.ast.list(&named);
        let app = t.ast.node(Tree::Apply { fun, args });
        t.ast.with_span(app, span);
        t.ast.add_attrs(app, Attrs::USING_APPLY);
        t.typed(app, Pt::Any, self.ctx)
    }
}
