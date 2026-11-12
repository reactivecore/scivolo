// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/backend/jvm/BCodeSkelBuilder.scala
//   compiler/src/dotty/tools/backend/jvm/BCodeHelpers.scala
//! A class's `serialVersionUID` field from `@SerialVersionUID` (dotty's `addSerialVUID`).

use super::locals::int_like;
use super::{Gen, unsupported};
use classfile::{ACC_FINAL, ACC_PRIVATE, ACC_STATIC, ClassFile};
use lower::Result;
use syntax::ast::constant::Constant;
use tasty::SymbolId;
use typer::tpd::{Tree, constant_of};

impl Gen<'_, '_> {
    /// Adds to `cf` the `serialVersionUID` field of class `c`, if it is no trait annotated
    /// `@SerialVersionUID`; refuses an argument that is no constant or constant expression.
    pub fn serial_version_uid(&mut self, cf: &mut ClassFile, c: SymbolId) -> Result<()> {
        if self.l.u.is_trait(c) {
            return Ok(());
        }
        for annot in self.l.tpd.annotations(c).to_vec() {
            let tpe = self.l.tpd.tpe(annot);
            if self.l.u.class_symbol(tpe).is_none_or(|s| self.l.u.full_name(s) != "scala.SerialVersionUID") {
                continue;
            }
            let arg = match *self.l.tpd.tree(annot) {
                Tree::Apply { args, .. } => self.l.tpd.items(args).first().copied(),
                _ => None,
            };
            let value = arg.and_then(|a| match self.l.tpd.tree(a) {
                Tree::Literal { value } => Some(value.clone()),
                _ => constant_of(self.l.u, self.l.tpd.tpe(a)),
            });
            let id = match value {
                Some(Constant::Long(v)) => v,
                Some(value) if let Some(v) = int_like(&value) => i64::from(v),
                _ => return unsupported(self.l.tpd.span(annot), "@SerialVersionUID of no constant"),
            };
            cf.field(ACC_PRIVATE | ACC_STATIC | ACC_FINAL, "serialVersionUID", "J");
            cf.long_constant_value(id);
        }
        Ok(())
    }
}
