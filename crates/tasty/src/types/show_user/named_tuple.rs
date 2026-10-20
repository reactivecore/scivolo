// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/printing/RefinedPrinter.scala

use super::{GLOBAL, Printer};
use crate::types::{Type, TypeId};

impl Printer<'_> {
    /// `(a : A, b : B)` if `tycon[args]` is `NamedTuple[("a", "b"), (A, B)]` (dotty's
    /// `toTextNamedTuple`).
    pub fn named_tuple(&mut self, tycon: TypeId, args: &[TypeId]) -> Option<String> {
        let &[names, values] = args else { return None };
        if self.full_name_of(tycon)? != "scala.NamedTuple.NamedTuple" {
            return None;
        }
        let names = self.elems_of(names)?;
        let values = self.elems_of(values)?;
        if names.len() != values.len() {
            return None;
        }
        let mut fields = Vec::with_capacity(names.len());
        for (n, v) in names.into_iter().zip(values) {
            let Type::Str(n) = self.u.types().get(n).clone() else { return None };
            fields.push(format!("{n} : {}", self.text(v, GLOBAL)));
        }
        Some(format!("({})", fields.join(", ")))
    }

    fn elems_of(&mut self, t: TypeId) -> Option<Vec<TypeId>> {
        let Type::Applied(c, args) = self.u.types().get(t).clone() else { return None };
        self.tuple_elems(c, &args)
    }
}
