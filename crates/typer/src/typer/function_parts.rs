//! Function and partial function types built from their parts, and their parameter types.

use super::Typer;
use tasty::{SymbolId, Type, TypeId};

impl Typer<'_> {
    /// The parameter types of `t` if it is a function type, else none.
    pub fn function_params(&mut self, t: TypeId) -> Vec<TypeId> {
        (0..=22).find_map(|n| self.function_parts(t, n)).map(|(ps, _)| ps).unwrap_or_default()
    }

    /// `FunctionN[parts]`, the last of `parts` the result type.
    pub fn function_of(&mut self, mut parts: Vec<TypeId>) -> Result<TypeId, &'static str> {
        let result = parts.pop().ok_or("function type")?;
        let class = self.function_class(parts.len()).ok_or("function of that many parameters")?;
        parts.push(result);
        Ok(self.u.intern_type(Type::Applied(class, parts)))
    }

    /// `PartialFunction[a, b]`.
    pub fn partial_function_of(&mut self, a: TypeId, b: TypeId) -> Result<TypeId, &'static str> {
        let class = self.resolve_full_name("scala.PartialFunction", true).ok_or("PartialFunction")?;
        let tycon = crate::defn::type_ref(self.u, class);
        Ok(self.u.intern_type(Type::Applied(tycon, vec![a, b])))
    }

    /// Whether `t` is a `PartialFunction` type.
    pub fn is_partial_function(&mut self, t: TypeId) -> bool {
        let t = self.zonk(t);
        let t = self.u.dealias(t);
        self.u.class_symbol(t).is_some_and(|c| self.is_partial_function_class(c))
    }

    pub fn is_partial_function_class(&self, c: SymbolId) -> bool {
        self.u.full_name(c) == "scala.PartialFunction"
    }
}
