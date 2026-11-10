// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Synthesizer.scala
//! Synthesized `TupledFunction[F, G]` (dotty's `synthesizedTupleFunction`):
//! `TupledFunctions.tupledFunctionN[F, G]`, `tupledFunctionXXL` above 22 parameters.

use super::Synthesizer;
use crate::tpd::TNodeId;
use crate::typer::Typer;
use crate::typer::small_tuples::MAX_TUPLE_ARITY;
use tasty::{Kind, Type, TypeId};

/// The parts of a `FunctionN` or `ContextFunctionN` type.
struct FunctionParts {
    params: Vec<TypeId>,
    result: TypeId,
    contextual: bool,
}

impl Synthesizer {
    /// Synthesizes `TupledFunction[fun, tupled]` if `tupled` is the tupled form of `fun`.
    pub fn tupled_function(
        self,
        t: &mut Typer<'_>,
        fun: TypeId,
        tupled: TypeId,
    ) -> Result<Option<TNodeId>, &'static str> {
        let at = self.at;
        let Some(arity) = tupled_arity(t, fun, tupled)? else { return Ok(None) };
        let name = match arity {
            ..=MAX_TUPLE_ARITY => format!("tupledFunction{arity}"),
            _ => "tupledFunctionXXL".to_owned(),
        };
        const MODULE: &str = "scala.runtime.TupledFunctions";
        let module = t.resolve_full_name(MODULE, false).ok_or(MODULE)?;
        let module_class = t.u.module_class(module).ok_or(MODULE)?;
        let member =
            t.u.decl(module_class, &name)
                .unwrap_or_default()
                .into_iter()
                .find(|&d| t.u.symbol(d).kind == Kind::Def)
                .ok_or("TupledFunctions member")?;
        let module_tpe = crate::defn::type_ref(t.u, module);
        let qualifier = t.build().ident(module, module_tpe, at);
        let select = t.build().select(qualifier, member, at);
        let targs = [t.type_tree(fun, at, false), t.type_tree(tupled, at, false)];
        let tpe = t.instantiated(select, &[fun, tupled])?;
        Ok(Some(t.build().type_apply(select, &targs, tpe, at)))
    }
}

/// The number of parameters of `fun` if `tupled` is its tupled form, constraining type
/// variables.
fn tupled_arity(t: &mut Typer<'_>, fun: TypeId, tupled: TypeId) -> Result<Option<usize>, &'static str> {
    let (fun, tupled) = (t.zonk(fun), t.zonk(tupled));
    if let Some(parts) = any_function_parts(t, fun) {
        let arg = tuple_type(t, &parts.params)?;
        let expected = function_n_of(t, vec![arg, parts.result], parts.contextual)?;
        return Ok(t.u.is_same_type_in(tupled, expected, &mut t.constraint).then_some(parts.params.len()));
    }
    let Some(parts) = any_function_parts(t, tupled) else { return Ok(None) };
    let [arg] = parts.params[..] else { return Ok(None) };
    let widened = t.u.widen(arg);
    let Some(mut elems) =
        t.tuple_class_args(widened).or_else(|| t.tuple_elements_within(widened, usize::MAX))
    else {
        return Ok(None);
    };
    let arity = elems.len();
    elems.push(parts.result);
    let expected = function_n_of(t, elems, parts.contextual)?;
    Ok(t.u.is_same_type_in(fun, expected, &mut t.constraint).then_some(arity))
}

/// The parts of `FunctionN` or `ContextFunctionN` type `tpe`.
fn any_function_parts(t: &mut Typer<'_>, tpe: TypeId) -> Option<FunctionParts> {
    if let Some((params, result)) = t.context_function_parts(tpe) {
        return Some(FunctionParts { params, result, contextual: true });
    }
    let d = t.u.dealias(tpe);
    let Type::Applied(_, args) = t.u.types().get(d) else { return None };
    let n = args.len().checked_sub(1)?;
    t.function_parts(tpe, n).map(|(params, result)| FunctionParts { params, result, contextual: false })
}

/// `FunctionN[parts]` or `ContextFunctionN[parts]`, the last of `parts` the result type.
fn function_n_of(t: &mut Typer<'_>, parts: Vec<TypeId>, contextual: bool) -> Result<TypeId, &'static str> {
    if !contextual {
        return t.function_of(parts);
    }
    let class = t.context_function_class(parts.len() - 1).ok_or("context function of that arity")?;
    Ok(t.u.intern_type(Type::Applied(class, parts)))
}

/// `TupleN[elems]`, or `e1 *: … *: EmptyTuple` for none or more than 22 (dotty's `tupleType`).
fn tuple_type(t: &mut Typer<'_>, elems: &[TypeId]) -> Result<TypeId, &'static str> {
    if elems.is_empty() || elems.len() > MAX_TUPLE_ARITY {
        return t.nested_pairs(elems);
    }
    let class = t.resolve_full_name(&format!("scala.Tuple{}", elems.len()), true).ok_or("TupleN")?;
    Ok(crate::defn::applied(t.u, class, elems.to_vec()))
}
