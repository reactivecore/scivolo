// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/reporting/messages.scala
//   compiler/src/dotty/tools/dotc/typer/Typer.scala
//! Applications of an overloaded method by its simple name that no alternative or several accept
//! (dotty's `NoMatchingOverload`, `AmbiguousOverload` from `adaptOverloaded`), reported where
//! dotty tries no conversion and the alternatives and arguments show as dotty shows them.

use super::{Pt, Typer};
use crate::tpd::TNodeId;
use syntax::ast::{NodeId, Tree};
use tasty::{Kind, SymbolId, Type, TypeId};

impl Typer<'_> {
    /// `NoMatchingOverload` for identifier `id` of methods `alts`, members of `prefix`, applied
    /// to the arguments of `pt`, or `AmbiguousOverload` if `ambiguous` of them match; `None`
    /// where dotty may word or decide it otherwise.
    pub fn overload_error(
        &mut self,
        id: NodeId,
        (alts, prefix): (&[SymbolId], Option<TypeId>),
        pt: Pt,
        ambiguous: Option<Vec<SymbolId>>,
    ) -> Option<TNodeId> {
        let Pt::Fun(proto) = pt else { return None };
        if !matches!(self.ast.tree(id), Tree::Ident { .. })
            || matches!(self.protos[proto].pt, Pt::Fun(_) | Pt::PolyFun(_))
            || ambiguous.is_some() && matches!(self.protos[proto].pt, Pt::Type(_))
            || self.protos[proto].dual.is_some()
            || alts.iter().any(|&s| self.u.symbol(s).kind != Kind::Def)
        {
            return None;
        }
        let owner = self.u.symbol(alts[0]).owner?;
        if alts.iter().any(|&s| self.u.symbol(s).owner != Some(owner)) {
            return None;
        }
        let tupled = self.protos[proto].args.len() > 1
            && alts.iter().all(|&s| {
                let info = self.u.info(s);
                self.u.param_clauses(info).first().is_some_and(|(_, ps)| ps.len() == 1)
            });
        if tupled {
            return None;
        }
        let args = self.overload_args_text(proto, alts, prefix)?;
        let mut types = Vec::new();
        for &s in alts.iter().rev().filter(|s| ambiguous.as_ref().is_none_or(|a| a.contains(s))) {
            let tpe = self.ref_type(prefix, s);
            let info = self.method_info(tpe)?;
            types.push(self.method_text(info)?);
        }
        let name = self.u.symbol(alts[0]).name.clone();
        let located = format!("method {name}{}", self.location(owner)?);
        let args = format!("({args})");
        let msg = match ambiguous {
            Some(_) => crate::typer::errors::ambiguous_overload(&located, &types, &args),
            None => crate::typer::errors::no_matching_overload(&located, &types, &args),
        };
        Some(self.error(id, msg))
    }

    /// The arguments of `proto` typed, as dotty's message shows their types.
    fn overload_args_text(
        &mut self,
        proto: usize,
        alts: &[SymbolId],
        prefix: Option<TypeId>,
    ) -> Option<String> {
        let ids = self.protos[proto].args.clone();
        if ids.iter().any(|&a| {
            matches!(self.ast.tree(a), Tree::NamedArg { .. } | Tree::Typed { .. } | Tree::Assign { .. })
                || self.unknown_param_function(a).is_some()
        }) {
            return None;
        }
        let typed = self.pretyped_args(proto, alts, prefix).ok()?;
        let mut texts = Vec::new();
        for a in typed {
            let t = self.tpd.tpe(a?);
            let t = self.zonk(t);
            if self.term_symbol(t).is_some_and(|s| self.u.symbol(s).kind == Kind::Def) {
                return None;
            }
            texts.push(self.found_text(t)?);
        }
        Some(texts.join(", "))
    }

    /// Method type `info` as dotty's message shows it, `(x: Int): Int`.
    fn method_text(&mut self, info: TypeId) -> Option<String> {
        for part in self.type_parts(info) {
            let known = match self.u.types().get(part) {
                Type::Method { .. } | Type::Poly { .. } | Type::Param(..) | Type::ByName(_) => true,
                Type::Constant(..) | Type::Str(_) => false,
                _ => crate::typer::not_a_member::is_concrete_part(self.u, part) && self.shown_as_dotty(part),
            };
            if !known {
                return None;
            }
        }
        if !self.distinct_names(&[info]) {
            return None;
        }
        Some(self.u.show_user(info)).filter(|text| !text.contains(['<', '?']))
    }

    /// The tree for reference `id` when `choose` finds no alternative for `what`: an error tree
    /// when `pt` is unusable for inference, else `what` unsupported.
    pub fn unchosen(&mut self, id: NodeId, what: &str, pt: Pt) -> TNodeId {
        if self.unusable_for_inference(pt) {
            return self.error_tree(self.ast.span(id));
        }
        self.unsupported(id, what)
    }

    /// Whether a typed argument of `pt` has the error type in it and that error is reported
    /// (dotty's `unusableForInference`).
    pub fn unusable_for_inference(&mut self, pt: Pt) -> bool {
        let Pt::Fun(proto) = pt else { return false };
        let args = self.protos[proto].typed.clone().unwrap_or_default();
        let reported = self.diags.has_errors() || self.unsupported.is_some();
        reported
            && args.into_iter().flatten().any(|a| {
                let tpe = self.tpd.tpe(a);
                let tpe = self.zonk(tpe);
                self.type_parts(tpe).into_iter().any(|p| *self.u.types().get(p) == Type::Error)
            })
    }
}
