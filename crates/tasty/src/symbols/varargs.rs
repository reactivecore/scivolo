//! The Java varargs of JDK methods, which javalib declares with a plain array parameter: their
//! last parameter typed `Array[T] @Repeated`, as dotty reads Java varargs.

use super::{Kind, SymbolId, Universe};
use crate::types::{Type, TypeId};

/// JDK methods with varargs that javalib has: class, method, number of parameters.
const VARARGS: [(&str, &str, usize); 19] = [
    ("java.io.PrintStream", "format", 2),
    ("java.io.PrintStream", "printf", 2),
    ("java.io.PrintWriter", "format", 2),
    ("java.io.PrintWriter", "printf", 2),
    ("java.lang.String", "format", 2),
    ("java.lang.String", "format", 3),
    ("java.lang.String", "formatted", 1),
    ("java.lang.String", "join", 2),
    ("java.nio.file.Path", "of", 2),
    ("java.nio.file.Paths", "get", 2),
    ("java.util.Arrays", "asList", 1),
    ("java.util.Collections", "addAll", 2),
    ("java.util.EnumSet", "of", 2),
    ("java.util.List", "of", 1),
    ("java.util.Map", "ofEntries", 1),
    ("java.util.Objects", "hash", 1),
    ("java.util.Set", "of", 1),
    ("java.util.stream.IntStream", "of", 1),
    ("java.util.stream.Stream", "of", 1),
];

impl Universe {
    /// Method type `t` of `def`, its last array parameter marked repeated if the JDK declares it
    /// as varargs.
    pub fn with_java_varargs(&mut self, def: SymbolId, t: TypeId) -> TypeId {
        let s = self.symbol(def);
        let Some(owner) = s.owner.filter(|_| s.kind == Kind::Def) else { return t };
        let (name, class) = (s.name.clone(), self.full_name(owner));
        let class = super::module_name(&class).unwrap_or(&class);
        let counts: Vec<usize> =
            VARARGS.iter().filter(|&&(c, m, _)| c == class && m == name).map(|&(_, _, n)| n).collect();
        if counts.is_empty() {
            return t;
        }
        self.mark_last_array(t, &counts)
    }

    /// `t` with the last parameter of its first clause marked repeated if that clause has one of
    /// `counts` parameters, the last an array.
    fn mark_last_array(&mut self, t: TypeId, counts: &[usize]) -> TypeId {
        match self.types.get(t).clone() {
            Type::Poly { binder, params, result } => {
                let result = self.mark_last_array(result, counts);
                self.types.intern(Type::Poly { binder, params, result })
            }
            Type::Method { binder, mut params, result, implicit, contextual }
                if counts.contains(&params.len()) =>
            {
                let last = params.last_mut().expect("a parameter");
                if self.is_array(last.1) {
                    let repeated = self.class_ref("scala.annotation.internal.Repeated");
                    last.1 = self.types.intern(Type::Annotated(last.1, repeated));
                }
                self.types.intern(Type::Method { binder, params, result, implicit, contextual })
            }
            _ => t,
        }
    }

    /// Whether `t` is `Array[e]`.
    pub fn is_array(&mut self, t: TypeId) -> bool {
        match self.types.get(t).clone() {
            Type::Applied(c, args) => {
                args.len() == 1 && self.class_symbol(c).is_some_and(|s| self.full_name(s) == "scala.Array")
            }
            _ => false,
        }
    }
}
