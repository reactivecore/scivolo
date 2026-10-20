// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Synthesizer.scala
//! Synthesized `TupledFunction[F, G]` (dotty's `synthesizedTupleFunction`):
//! `TupledFunctions.tupledFunctionN[F, G]`, `tupledFunctionXXL` above 22 parameters.

use super::super::Typer;
use super::super::small_tuples::MAX_TUPLE_ARITY;
use crate::tpd::TNodeId;
use syntax::ast::Span;
use tasty::{Kind, Type, TypeId};

impl Typer<'_> {
    pub fn synthesized_tupled_function(
        &mut self,
        fun: TypeId,
        tupled: TypeId,
        at: Span,
    ) -> Result<Option<TNodeId>, &'static str> {
        let Some(arity) = self.tupled_arity(fun, tupled)? else { return Ok(None) };
        let name = match arity {
            ..=MAX_TUPLE_ARITY => format!("tupledFunction{arity}"),
            _ => "tupledFunctionXXL".to_owned(),
        };
        const MODULE: &str = "scala.runtime.TupledFunctions";
        let module = self.resolve_full_name(MODULE, false).ok_or(MODULE)?;
        let module_class = self.u.module_class(module).ok_or(MODULE)?;
        let member = self
            .u
            .decl(module_class, &name)
            .unwrap_or_default()
            .into_iter()
            .find(|&d| self.u.symbol(d).kind == Kind::Def)
            .ok_or("TupledFunctions member")?;
        let module_tpe = crate::defn::type_ref(self.u, module);
        let qualifier = self.build().ident(module, module_tpe, at);
        let select = self.build().select(qualifier, member, at);
        let targs = [self.type_tree(fun, at, false), self.type_tree(tupled, at, false)];
        let tpe = self.instantiated(select, &[fun, tupled])?;
        Ok(Some(self.build().type_apply(select, &targs, tpe, at)))
    }

    /// The number of parameters of `fun` if `tupled` is its tupled form, constraining type
    /// variables.
    fn tupled_arity(&mut self, fun: TypeId, tupled: TypeId) -> Result<Option<usize>, &'static str> {
        let (fun, tupled) = (self.zonk(fun), self.zonk(tupled));
        if let Some((params, result, contextual)) = self.any_function_parts(fun) {
            let arg = self.tuple_type(&params)?;
            let expected = self.function_n_of(vec![arg, result], contextual)?;
            return Ok(self
                .u
                .is_same_type_in(tupled, expected, &mut self.constraint)
                .then_some(params.len()));
        }
        let Some((params, result, contextual)) = self.any_function_parts(tupled) else { return Ok(None) };
        let [arg] = params[..] else { return Ok(None) };
        let widened = self.u.widen(arg);
        let Some(mut elems) =
            self.tuple_class_args(widened).or_else(|| self.tuple_elements_within(widened, usize::MAX))
        else {
            return Ok(None);
        };
        let arity = elems.len();
        elems.push(result);
        let expected = self.function_n_of(elems, contextual)?;
        Ok(self.u.is_same_type_in(fun, expected, &mut self.constraint).then_some(arity))
    }

    /// The parameter and result types of `FunctionN` or `ContextFunctionN` type `t`, and whether
    /// it is a context function.
    fn any_function_parts(&mut self, t: TypeId) -> Option<(Vec<TypeId>, TypeId, bool)> {
        if let Some((params, result)) = self.context_function_parts(t) {
            return Some((params, result, true));
        }
        let d = self.u.dealias(t);
        let Type::Applied(_, args) = self.u.types().get(d) else { return None };
        let n = args.len().checked_sub(1)?;
        self.function_parts(t, n).map(|(params, result)| (params, result, false))
    }

    /// `FunctionN[parts]` or `ContextFunctionN[parts]`, the last of `parts` the result type.
    fn function_n_of(&mut self, parts: Vec<TypeId>, contextual: bool) -> Result<TypeId, &'static str> {
        if !contextual {
            return self.function_of(parts);
        }
        let class = self.context_function_class(parts.len() - 1).ok_or("context function of that arity")?;
        Ok(self.u.intern_type(Type::Applied(class, parts)))
    }

    /// `TupleN[elems]`, or `e1 *: … *: EmptyTuple` for none or more than 22 (dotty's `tupleType`).
    fn tuple_type(&mut self, elems: &[TypeId]) -> Result<TypeId, &'static str> {
        if elems.is_empty() || elems.len() > MAX_TUPLE_ARITY {
            return self.nested_pairs(elems);
        }
        let class = self.resolve_full_name(&format!("scala.Tuple{}", elems.len()), true).ok_or("TupleN")?;
        Ok(crate::defn::applied(self.u, class, elems.to_vec()))
    }
}
