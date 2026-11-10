// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/reporting/messages.scala
//   compiler/src/dotty/tools/dotc/typer/Typer.scala
//! Applications of an overloaded method by its simple name that no alternative or several accept
//! (dotty's `NoMatchingOverload`, `AmbiguousOverload` from `adaptOverloaded`), reported where
//! dotty tries no conversion and the alternatives and arguments show as dotty shows them.

use super::Resolution;
use super::shapes::unknown_param_function;
use crate::tpd::TNodeId;
use crate::typer::{Pt, Typer};
use syntax::ast::{NodeId, Tree};
use tasty::{Kind, SymbolId, Type, TypeId};

impl Resolution {
    /// Reports `NoMatchingOverload` for identifier `id` of methods `alts` applied to the
    /// arguments of the expected type, as `f(true)` among `def f(x: Int)` and `def f(x: String)`,
    /// or `AmbiguousOverload` if `ambiguous` of them match; `None` where dotty may word or decide
    /// it otherwise.
    pub fn overload_error(
        self,
        t: &mut Typer,
        id: NodeId,
        alts: &[SymbolId],
        ambiguous: Option<Vec<SymbolId>>,
    ) -> Option<TNodeId> {
        let Pt::Fun(proto) = self.pt else { return None };
        if !matches!(t.ast.tree(id), Tree::Ident { .. })
            || matches!(t.protos[proto].pt, Pt::Fun(_) | Pt::PolyFun(_))
            || ambiguous.is_some() && matches!(t.protos[proto].pt, Pt::Type(_))
            || t.protos[proto].dual.is_some()
            || alts.iter().any(|&s| t.u.symbol(s).kind != Kind::Def)
        {
            return None;
        }
        let owner = t.u.symbol(alts[0]).owner?;
        if alts.iter().any(|&s| t.u.symbol(s).owner != Some(owner)) {
            return None;
        }
        let tupled = t.protos[proto].args.len() > 1
            && alts.iter().all(|&s| {
                let info = t.u.info(s);
                t.u.param_clauses(info).first().is_some_and(|(_, ps)| ps.len() == 1)
            });
        if tupled {
            return None;
        }

        let args = self.overload_args_text(t, proto, alts)?;
        let mut types = Vec::new();
        for &s in alts.iter().rev().filter(|s| ambiguous.as_ref().is_none_or(|a| a.contains(s))) {
            let tpe = t.ref_type(self.prefix, s);
            let info = t.method_info(tpe)?;
            types.push(method_text(t, info)?);
        }
        let name = t.u.symbol(alts[0]).name.clone();
        let located = format!("method {name}{}", t.location(owner)?);
        let args = format!("({args})");
        let msg = match ambiguous {
            Some(_) => crate::typer::errors::ambiguous_overload(&located, &types, &args),
            None => crate::typer::errors::no_matching_overload(&located, &types, &args),
        };
        Some(t.error(id, msg))
    }

    /// Shows the types of the arguments of `proto`, typed, as dotty's message shows them.
    fn overload_args_text(self, t: &mut Typer, proto: usize, alts: &[SymbolId]) -> Option<String> {
        let ids = t.protos[proto].args.clone();
        if ids.iter().any(|&a| {
            matches!(t.ast.tree(a), Tree::NamedArg { .. } | Tree::Typed { .. } | Tree::Assign { .. })
                || unknown_param_function(t, a).is_some()
        }) {
            return None;
        }
        let typed = self.pretyped_args(t, proto, alts).ok()?;
        let mut texts = Vec::new();
        for a in typed {
            let tpe = t.tpd.tpe(a?);
            let tpe = t.zonk(tpe);
            if t.term_symbol(tpe).is_some_and(|s| t.u.symbol(s).kind == Kind::Def) {
                return None;
            }
            texts.push(t.found_text(tpe)?);
        }
        Some(texts.join(", "))
    }
}

/// Shows method type `info` as dotty's message shows it, `(x: Int): Int`.
fn method_text(t: &mut Typer, info: TypeId) -> Option<String> {
    for part in t.type_parts(info) {
        let known = match t.u.types().get(part) {
            Type::Method { .. } | Type::Poly { .. } | Type::Param(..) | Type::ByName(_) => true,
            Type::Constant(..) | Type::Str(_) => false,
            _ => crate::typer::not_a_member::is_concrete_part(t.u, part) && t.shown_as_dotty(part),
        };
        if !known {
            return None;
        }
    }
    if !t.distinct_names(&[info]) {
        return None;
    }
    Some(t.u.show_user(info)).filter(|text| !text.contains(['<', '?']))
}
