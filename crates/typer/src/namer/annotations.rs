use crate::typer::Typer;
use syntax::ast::{NodeId, Tree};
use tasty::SymbolId;

/// Annotations without arguments that change neither typing nor warnings.
const HINTS: [&str; 5] = [
    "scala.noinline",
    "scala.inline",
    "scala.annotation.tailrec",
    "scala.annotation.unused",
    "scala.annotation.switch",
];

impl Typer<'_> {
    /// Checks that annotation `annot` of class `class` is one the typer handles: `@main`, or a
    /// hint without arguments.
    pub fn check_annotation(&mut self, annot: NodeId, class: Option<SymbolId>) {
        let mut t = annot;
        let mut no_args = true;
        while let Tree::Apply { fun, args } = *self.ast.tree(t) {
            no_args &= self.ast.items(args).is_empty();
            t = fun;
        }
        let hint = class.is_some_and(|c| HINTS.contains(&self.u.full_name(c).as_str())) && no_args;
        if class != Some(self.defn.main_annot) && !hint {
            self.unsupported(annot, "annotation");
        }
    }
}
