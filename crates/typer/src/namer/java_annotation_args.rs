// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Applications.scala
//   compiler/src/dotty/tools/dotc/typer/Checking.scala

use crate::tpd::{TNodeId, Tree as T};
use crate::typer::Typer;
use names::nme;
use syntax::ast::{NodeId, TName, Tree};
use tasty::{NameId, SymbolId, TypeId};

impl Typer<'_> {
    /// Names the arguments of annotation `annot` of class `class` if `class` is a Java annotation
    /// class, a class file's or the JDK's (dotty's `Checking.checkNamedArgumentForJavaAnnotation`):
    /// `@SuppressWarnings(xs)` becomes `@SuppressWarnings(value = xs)` where the class has a
    /// member `value`; reports every other positional argument, as in `@A(1, 2)`.
    pub fn check_java_annotation_args(&mut self, annot: NodeId, class: SymbolId) {
        if !self.u.is_java_defined(class) && !self.u.enter_jdk_annotation(class) {
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

    /// Whether `method` is the constructor of a class file's annotation class (dotty's
    /// `isJavaAnnotConstr`).
    pub fn is_java_annotation_constructor(&self, method: SymbolId) -> bool {
        let owner = self.u.symbol(method).owner;
        self.u.symbol(method).name == nme::CONSTRUCTOR
            && owner.is_some_and(|c| self.u.classfile_retention(c).is_some())
    }

    /// The placeholder `_` of the parameter's type for parameter `n` of `method` if it is the
    /// constructor of a class file's annotation class, as in `@Deprecated`, a refusal if its
    /// element has no default value (dotty's `tryDefault` for `isJavaAnnotConstr`).
    pub fn java_annotation_default(
        &mut self,
        f: TNodeId,
        (method, n): (SymbolId, usize),
    ) -> Option<Result<TNodeId, &'static str>> {
        if !self.is_java_annotation_constructor(method) {
            return None;
        }
        let owner = self.u.symbol(method).owner?;
        let info = self.u.info(method);
        let &(name, formal) = self.u.method(info)?.params.get(n)?;
        let name = self.u.name(name).to_owned();
        if !self.u.has_annotation_default(owner, &name) {
            return Some(Err("missing argument of a Java annotation"));
        }
        let underscore = TName { name: self.name(nme::WILDCARD), is_type: false };
        let span = self.tpd.span(f).to_synthetic();
        Some(Ok(self.tpd.node(T::Ident { name: underscore }, span, Some(formal), None)))
    }

    /// Arguments `typed` of a Java annotation's constructor of parameters `params`, each named
    /// (dotty's `ApplyToUntyped.result` for `isJavaAnnotConstr`, #19951).
    pub fn named_java_annotation_args(
        &mut self,
        params: &[(NameId, TypeId)],
        typed: Vec<TNodeId>,
    ) -> Vec<TNodeId> {
        let named = typed.into_iter().zip(params).map(|(arg, &(name, _))| {
            if matches!(self.tpd.tree(arg), T::NamedArg { .. }) {
                return arg;
            }
            let name = TName { name: self.name(&self.u.name(name).to_owned()), is_type: false };
            let (span, tpe) = (self.tpd.span(arg), self.tpd.get(arg).tpe);
            self.tpd.node(T::NamedArg { name, arg }, span, tpe, None)
        });
        named.collect()
    }
}
