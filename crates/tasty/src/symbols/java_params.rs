//! The parameter names of JDK methods, which the JDK's class files lack: `x$0`, `x$1`, … as dotty
//! names them.

use super::{Kind, SymbolId, Universe};
use crate::types::{Type, TypeId};

impl Universe {
    /// Type `t` of method `def` with its term parameters named by position if `def` is a member
    /// of a Java class of a `java.*` or `javax.*` package.
    pub fn with_java_param_names(&mut self, def: SymbolId, t: TypeId) -> TypeId {
        let Some(owner) = self.symbol(def).owner else { return t };
        let class = self.full_name(owner);
        let jdk = class.starts_with("java.") || class.starts_with("javax.");
        if self.symbol(def).kind != Kind::Def || !jdk || !self.may_be_java(owner) {
            return t;
        }
        self.positional_params(t)
    }

    fn positional_params(&mut self, t: TypeId) -> TypeId {
        match self.ty(t).clone() {
            Type::Poly { binder, params, result } => {
                let result = self.positional_params(result);
                self.intern_type(Type::Poly { binder, params, result })
            }
            Type::Method { binder, params, result, implicit, contextual } => {
                let params = params
                    .into_iter()
                    .enumerate()
                    .map(|(i, (_, p))| (self.intern_name(&format!("x${i}")), p))
                    .collect();
                let result = self.positional_params(result);
                self.intern_type(Type::Method { binder, params, result, implicit, contextual })
            }
            _ => t,
        }
    }
}
