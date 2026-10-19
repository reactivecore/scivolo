// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Applications.scala
//   compiler/src/dotty/tools/dotc/typer/ProtoTypes.scala
//! The arguments of an overloaded application typed before an alternative is chosen, functions
//! without parameter types against the parameter types all alternatives agree on (dotty's
//! `pretypeArgs`, `FunProto.typedArgs`).

use super::{Pt, Typer};
use crate::tpd::TNodeId;
use syntax::ast::{NodeId, Tree};
use tasty::{ANY, NOTHING, SymbolId, Type, TypeId};

impl Typer<'_> {
    /// The arguments of `proto`, typed once: a function without parameter types against the
    /// function type of the parameter types all of `alts` agree on at its position, else left
    /// for the chosen alternative (`None`); the others without an expected type.
    pub fn pretyped_args(
        &mut self,
        proto: usize,
        alts: &[SymbolId],
        prefix: Option<TypeId>,
    ) -> Result<Vec<Option<TNodeId>>, &'static str> {
        if let Some(typed) = &self.protos[proto].typed {
            return Ok(typed.clone());
        }
        let (args, ctx) = (self.protos[proto].args.clone(), self.protos[proto].ctx);
        let mut typed = Vec::new();
        for (i, &a) in args.iter().enumerate() {
            typed.push(match self.unknown_param_function(a) {
                Some(f) => {
                    self.common_formal(f, i, alts, prefix)?.map(|p| self.typed_unmapped(a, Pt::Type(p), ctx))
                }
                None => Some(self.typed_unmapped(a, Pt::Any, ctx)),
            });
        }
        self.protos[proto].typed = Some(typed.clone());
        Ok(typed)
    }

    /// For function `f` without parameter types at position `i`, the function or partial
    /// function type of the parameter types all of `alts` agree on there.
    fn common_formal(
        &mut self,
        f: NodeId,
        i: usize,
        alts: &[SymbolId],
        prefix: Option<TypeId>,
    ) -> Result<Option<TypeId>, &'static str> {
        if matches!(self.ast.tree(f), Tree::Function { mods: Some(_), .. }) {
            return Err("function argument with modifiers of an overload");
        }
        let mut common: Option<Vec<TypeId>> = None;
        let mut partial = false;
        for &alt in alts {
            let Some(formal) = self.formal_at(alt, i, prefix) else { return Ok(None) };
            let pf = self.partial_function_arg(formal);
            partial |= pf.is_some();
            let ps = pf.map_or_else(|| self.function_params(formal), |a| vec![a]);
            match &common {
                _ if ps.is_empty() => return Ok(None),
                None => common = Some(ps),
                Some(c) => {
                    let same = c.len() == ps.len()
                        && c.iter()
                            .zip(&ps)
                            .all(|(&a, &b)| self.u.is_subtype(a, b) && self.u.is_subtype(b, a));
                    if !same {
                        return Ok(None);
                    }
                }
            }
        }
        let Some(common) = common else { return Ok(None) };
        let mut params = Vec::new();
        for p in common {
            params.push(self.fully_defined(p)?);
        }
        let unknown = self.u.intern_type(Type::Wildcard(None));
        if partial && matches!(self.ast.tree(f), Tree::Match { .. }) {
            return self.partial_function_of(params[0], unknown).map(Some);
        }
        params.push(unknown);
        self.function_of(params).map(Some)
    }

    /// The type of argument `arg` of no type yet while alternatives are compared: a function of
    /// as many parameters of unknown types, of an unknown result; a case lambda of an unknown
    /// type (dotty's `typedArgs` without forcing).
    pub fn untyped_arg_type(&mut self, arg: NodeId) -> Result<TypeId, &'static str> {
        let f = self.unknown_param_function(arg).ok_or("argument of an overload")?;
        match *self.ast.tree(f) {
            Tree::Function { args, .. } => {
                let params = self.ast.items(args).to_vec();
                if params.iter().any(|&p| !self.is_untyped_param(p)) {
                    return Err("function argument of an overload with some parameter types");
                }
                let mut parts = vec![ANY; params.len()];
                parts.push(NOTHING);
                self.function_of(parts)
            }
            _ => Ok(self.u.intern_type(Type::Wildcard(None))),
        }
    }

    /// The argument type `A` of `t` if it is a `PartialFunction[A, B]`.
    fn partial_function_arg(&mut self, t: TypeId) -> Option<TypeId> {
        if !self.is_partial_function(t) {
            return None;
        }
        let t = self.zonk(t);
        let t = self.u.dealias(t);
        let Type::Applied(_, args) = self.u.types().get(t).clone() else { return None };
        (args.len() == 2).then(|| args[0])
    }
}
