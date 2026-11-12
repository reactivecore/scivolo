// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Checking.scala

use crate::typer::Typer;
use names::nme;
use syntax::ast::{NodeId, TName, Tree};
use tasty::SymbolId;

impl Typer<'_> {
    /// Names the arguments of annotation `annot` of class `class` if `class` is a Java annotation
    /// class (dotty's `Checking.checkNamedArgumentForJavaAnnotation`): `@SuppressWarnings(xs)`
    /// becomes `@SuppressWarnings(value = xs)` where the class has a member `value`; reports
    /// every other positional argument, as in `@A(1, 2)`.
    pub fn check_java_annotation_args(&mut self, annot: NodeId, class: SymbolId) {
        if !self.u.is_java_defined(class) {
            return;
        }
        let Tree::Apply { fun, args } = *self.ast.tree(annot) else { return };
        let params = self.ast.items(args).to_vec();
        let positional: Vec<NodeId> =
            params.iter().copied().filter(|&p| !matches!(self.ast.tree(p), Tree::NamedArg { .. })).collect();

        if let [param] = params[..]
            && positional == [param]
            && !self.u.decls_named(class, nme::VALUE).is_empty()
        {
            let name = TName { name: self.name(nme::VALUE), is_type: false };
            let named = self.ast.node(Tree::NamedArg { name, arg: param });
            let args = self.ast.list(&[named]);
            self.ast.get_mut(annot).tree = Tree::Apply { fun, args };
            return;
        }

        for param in positional {
            let msg = "Named arguments are required for Java defined annotations".to_owned()
                + &self.rewrite_notice("This", "3.6-migration");
            self.error(param, msg);
        }
    }
}
