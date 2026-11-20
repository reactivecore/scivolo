//! Function and partial function types built from their parts, and their parameter types.

use super::Typer;
use tasty::defn::PARTIAL_FUNCTION;
use tasty::{MAX_IMPLEMENTED_ARITY, SymbolId, TypeId};

impl Typer<'_> {
    /// Finds the parameter types of `t` if it is a function type, else none.
    pub fn function_params(&mut self, t: TypeId) -> Vec<TypeId> {
        (0..=MAX_IMPLEMENTED_ARITY)
            .find_map(|n| self.function_parts(t, n))
            .map(|(ps, _)| ps)
            .unwrap_or_default()
    }

    /// Finds the parameter types of `t` if it is a function or context function type (dotty's
    /// `FunctionOf`), else none.
    pub fn any_function_params(&mut self, t: TypeId) -> Vec<TypeId> {
        match self.context_function_parts(t) {
            Some((ps, _)) => ps,
            None => self.function_params(t),
        }
    }

    /// Builds `FunctionN[parts]`, the last of `parts` the result type.
    pub fn function_of(&mut self, parts: Vec<TypeId>) -> Result<TypeId, &'static str> {
        self.function_of_kind(parts, false)
    }

    /// Builds `FunctionN[parts]`, or `ContextFunctionN[parts]` if `contextual`.
    pub fn function_of_kind(
        &mut self,
        mut parts: Vec<TypeId>,
        contextual: bool,
    ) -> Result<TypeId, &'static str> {
        let result = parts.pop().ok_or("function type")?;
        let n = parts.len();
        let class = if contextual { self.context_function_class(n) } else { self.function_class(n) };
        let class = class.ok_or("function of that many parameters")?;
        parts.push(result);
        Ok(self.u.intern_applied(class, &parts))
    }

    /// Builds `PartialFunction[a, b]`.
    pub fn partial_function_of(&mut self, a: TypeId, b: TypeId) -> Result<TypeId, &'static str> {
        let class = self.resolve_full_name(PARTIAL_FUNCTION, true).ok_or("PartialFunction")?;
        let tycon = crate::defn::type_ref(self.u, class);
        Ok(self.u.intern_applied(tycon, &[a, b]))
    }

    /// Checks whether `t` is a `PartialFunction` type.
    pub fn is_partial_function(&mut self, t: TypeId) -> bool {
        let t = self.zonk(t);
        let t = self.u.dealias(t);
        self.u.class_symbol(t).is_some_and(|c| self.is_partial_function_class(c))
    }

    /// Checks whether class `c` is `PartialFunction`.
    pub fn is_partial_function_class(&self, c: SymbolId) -> bool {
        self.u.full_name(c) == PARTIAL_FUNCTION
    }
}
