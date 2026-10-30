// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Typer.scala
//! Method references eta-expanded against a single abstract method type (dotty's `adaptNoArgs`).

use super::Typer;
use crate::tpd::TNodeId;
use tasty::TypeId;

impl Typer<'_> {
    /// Whether `pt` is a single abstract method type of `n` parameters, with its parent if its
    /// class is not `@FunctionalInterface`; `Err` for a Java class other than a JDK functional
    /// interface.
    pub fn sam_method_value(&mut self, pt: TypeId, n: usize) -> Result<(bool, Option<TypeId>), &'static str> {
        let outside = "method value of a single abstract method type";
        let Some(sam) = self.sam_parts(pt, n, true).map_err(|_| outside)? else { return Ok((false, None)) };
        let class = self.u.class_symbol(pt).ok_or(outside)?;
        let java = self.is_java_class_name(class);
        if java && !super::java_classes::is_functional_interface(&self.u.full_name(class)) {
            return Err(outside);
        }
        let annotated =
            self.u.annotations(class).iter().any(|&a| self.u.full_name(a) == "java.lang.FunctionalInterface");
        Ok((true, (!java && !annotated).then_some(sam.parent)))
    }

    /// Warns that method reference `tree` is eta-expanded to `parent`, of a class not
    /// `@FunctionalInterface`.
    pub fn warn_not_functional_interface(&mut self, tree: TNodeId, parent: TypeId) {
        let name = self.tpd.sym(tree).map_or_else(String::new, |s| self.u.symbol(s).name.clone());
        let parent = self.zonk(parent);
        let parent = self.u.show_user(parent);
        let msg = format!(
            "method {name} is eta-expanded even though {parent} does not have the @FunctionalInterface annotation."
        );
        let span = self.tpd.span(tree);
        self.diags.warning_at(span.start(), span.end(), span.point(), msg);
    }
}
