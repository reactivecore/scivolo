// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Applications.scala
//   compiler/src/dotty/tools/dotc/typer/Typer.scala
//! Function arguments with parameter types taking their parameters as one tuple while overloads
//! are compared (dotty's `normArg` with `isCorrectUnaryFunction`, `ptIsCorrectProduct`).

use super::Resolution;
use super::shapes::{formal_at, unblocked_all};
use crate::ctx::Ctx;
use crate::defn::TUPLE;
use crate::typer::Typer;
use syntax::ast::{NodeId, Tree};
use tasty::{SymbolId, Type, TypeId};

/// Why a function argument with parameter types is not resolved when only some alternatives
/// take it as a function of one tuple, as `(x: Int, s: String) => x` against `Int => Int` and
/// `((Int, String)) => Int`.
const TUPLED_BESIDE_OTHERS: &str = "tupled function argument with parameter types beside other formals";

impl Resolution {
    /// Finds the type `((A, B)) => ?` that function argument `arg` at position `i`, of parameters
    /// all with types `A` and `B`, is typed against while `alts` are compared: when no
    /// alternative takes a function of as many parameters there and one takes a function of a
    /// tuple of as many elements, each conforming to its parameter's type, as
    /// `(x: Int, y: Int) => x + y` against `((Int, Int)) => Int`; refuses a tuple that is a type
    /// parameter of the alternative, and alternatives of which only some take such a function.
    pub fn tupled_function_pt(
        self,
        t: &mut Typer,
        (arg, i): (NodeId, usize),
        alts: &[SymbolId],
        ctx: Ctx,
    ) -> Result<Option<TypeId>, &'static str> {
        let arg = unblocked_all(t.ast, arg);
        let Tree::Function { args, mods: None, .. } = *t.ast.tree(arg) else { return Ok(None) };
        let params = t.ast.items(args).to_vec();
        if params.len() < 2 || params.iter().any(|&p| t.is_untyped_param(p)) {
            return Ok(None);
        }

        let mut unary = Vec::new();
        for &alt in alts {
            let ps = formal_at(t, alt, i, self.prefix).map_or_else(Vec::new, |f| t.function_params(f));
            if ps.len() == params.len() {
                return Ok(None);
            }
            if let [p] = ps[..] {
                unary.push((alt, p));
            }
        }
        if unary.is_empty() {
            return Ok(None);
        }

        let mut declared = Vec::new();
        for &p in &params {
            let Tree::ValDef { tpt, .. } = *t.ast.tree(p) else { return Ok(None) };
            let typed = t.typed_type(tpt, ctx);
            declared.push(t.tpd.tpe(typed));
        }
        let mut fitting = 0;
        for &(alt, p) in &unary {
            fitting += usize::from(correct_product(t, alt, p, &declared)?);
        }
        match fitting {
            0 => return Ok(None),
            n if n < alts.len() => return Err(TUPLED_BESIDE_OTHERS),
            _ => {}
        }

        let tuple = t.resolve_full_name(&format!("{TUPLE}{}", params.len()), true).ok_or("tuple class")?;
        let tuple = crate::defn::applied(t.u, tuple, declared);
        let unknown = t.u.intern_type(Type::Wildcard(None));
        t.function_of(vec![tuple, unknown]).map(Some)
    }
}

/// Checks whether parameter type `p` of a unary function formal of `alt` is a tuple of as many
/// elements as `declared`, each conforming to its type (dotty's `ptIsCorrectProduct`).
fn correct_product(
    t: &mut Typer,
    alt: SymbolId,
    p: TypeId,
    declared: &[TypeId],
) -> Result<bool, &'static str> {
    let p = t.zonk(p);
    if t.u.sym_of(p).is_some_and(|s| t.u.symbol(s).owner == Some(alt)) {
        return Err("tupled function argument against a type parameter");
    }
    let Ok(p) = t.fully_defined(p) else { return Ok(false) };
    let tpe = t.u.dealias(p);
    let Some((tycon, elems)) = t.u.applied(tpe).map(|(c, e)| (c, e.to_vec())) else { return Ok(false) };
    let n = declared.len();
    if elems.len() != n || !t.u.is_class_named(tycon, &format!("{TUPLE}{n}")) {
        return Ok(false);
    }
    Ok(elems.iter().zip(declared).all(|(&e, &d)| t.u.is_subtype(e, d)))
}
