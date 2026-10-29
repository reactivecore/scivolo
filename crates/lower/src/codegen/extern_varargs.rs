// Derived from Scala Native 0.5.12 (Apache-2.0, see native/LICENSE-scala-native.md); translated to Rust and changed:
//   nscplugin/src/main/scala-3/scala/scalanative/nscplugin/NirGenType.scala

use super::Gen;
use tasty::{SymbolId, Type};

impl Gen<'_, '_> {
    pub fn extern_vararg_index(&mut self, method: SymbolId) -> Option<usize> {
        let mut t = self.l.u.info(method);
        let mut index = 0;
        loop {
            match self.l.u.types().get(t).clone() {
                Type::Poly { result, .. } => t = result,
                Type::Method { params, result, .. } => {
                    for (_, param) in params {
                        if self.l.u.is_erased_param(param) {
                            continue;
                        }
                        let is_repeated = match self.l.u.types().get(param).clone() {
                            Type::Applied(c, _) => self
                                .l
                                .u
                                .class_symbol(c)
                                .is_some_and(|s| self.l.u.full_name(s) == "scala.<repeated>"),
                            Type::Annotated(_, a) => self.l.u.class_symbol(a).is_some_and(|s| {
                                self.l.u.full_name(s) == "scala.annotation.internal.Repeated"
                            }),
                            _ => false,
                        };
                        if is_repeated {
                            return Some(index);
                        }
                        index += 1;
                    }
                    t = result;
                }
                _ => return None,
            }
        }
    }
}
