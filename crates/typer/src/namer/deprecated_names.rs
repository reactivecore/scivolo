// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Typer.scala

use crate::ctx::Ctx;
use crate::typer::Typer;
use names::nme;
use syntax::ast::{NodeId, Tree};
use tasty::SymbolId;

impl Typer<'_> {
    /// Refuses parameter clause `params` of a method if a `@deprecatedName` repeats a name of an
    /// earlier parameter or deprecated name, or a parameter repeats an earlier deprecated name
    /// (dotty's `typedDefDef` checks).
    pub fn check_deprecated_names(&mut self, params: &[SymbolId]) {
        const DEPRECATED_NAME: &str = "scala.deprecatedName";
        if !params.iter().any(|&p| self.u.is_annotated(p, DEPRECATED_NAME)) {
            return;
        }
        let mut seen: Vec<String> = Vec::new();
        for &p in params {
            let name = self.u.symbol(p).name.clone();
            let tree = self.syms.param_trees.get(&p).copied();
            if seen.contains(&name)
                && let Some(tree) = tree
            {
                self.unsupported(tree, "parameter name equal to a deprecated name");
            }
            let annots: Vec<(NodeId, Ctx)> =
                self.syms.annotation_trees.iter().filter(|a| a.0 == p).map(|a| (a.1, a.2)).collect();
            for (annot, ctx) in annots {
                if self
                    .annotation_class(annot, ctx)
                    .is_none_or(|(c, _)| self.u.full_name(c) != DEPRECATED_NAME)
                {
                    continue;
                }
                let deprecated = match self.deprecated_name_arg(annot) {
                    Some(arg) => self.string_value(arg).unwrap_or_default(),
                    None => nme::NO_NAME.to_owned(),
                };
                if seen.contains(&deprecated) {
                    self.unsupported(annot, "deprecated parameter name equal to another name");
                }
                seen.push(deprecated);
            }
            seen.push(name);
        }
    }

    /// The `name` argument of `@deprecatedName` annotation `annot`: its first positional argument
    /// or the one named `name`.
    fn deprecated_name_arg(&self, annot: NodeId) -> Option<NodeId> {
        let Tree::Apply { args, .. } = *self.ast.tree(annot) else { return None };
        let args = self.ast.items(args);
        args.iter().copied().find(|&a| match *self.ast.tree(a) {
            Tree::NamedArg { name, .. } => self.text(name.name) == nme::NAME,
            _ => a == args[0],
        })
    }
}
