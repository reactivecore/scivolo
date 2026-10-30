// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/Types.scala
//! The function type of a closure without a target (dotty's `toFunctionType`).

use super::{Refusal, Unpickler};
use crate::tpd::TNodeId;
use crate::typer::approx::{self, Approx};
use crate::typer::dependent_params::mentions_params;
use tasty::{Binder, Kind, NOTHING, SymbolId, Type, TypeId};

/// The method type whose dependent result is approximated: its binder, parameter types and
/// method symbol.
type Params<'a> = (Binder, &'a [TypeId], SymbolId);

impl Unpickler<'_> {
    /// The function type of the closure of local method `meth`.
    pub fn function_type(&mut self, meth: TNodeId) -> Result<TypeId, Refusal> {
        let s = self.tpd.sym(meth).ok_or("closure of an unknown method in a library inline body")?;
        let info = self.u.info(s);
        let Type::Method { binder, params, result, contextual, .. } = self.u.types().get(info).clone() else {
            return Err("closure of a method without parameters in a library inline body");
        };
        if self.u.has_erased_params(info) {
            let poly_function = self.u.class_ref("scala.PolyFunction");
            return Ok(self.u.intern_type(Type::Refined(poly_function, "apply".into(), info)));
        }
        let types: Vec<TypeId> = params.iter().map(|&(_, p)| p).collect();
        let dependent = mentions_params(self.u, result, binder, Some(s));
        let approx = match dependent {
            true => self.approx(result, 1, (binder, &types, s)),
            false => Some(Approx::Exact(result)),
        };
        let approx = approx.ok_or("dependent closure in a library inline body")?.hi();
        let kind = if contextual { "ContextFunction" } else { "Function" };
        let class = self.class_symbol(&format!("scala.{kind}{}", params.len()))?;
        let function = crate::defn::applied(self.u, class, types.into_iter().chain([approx]).collect());
        Ok(match dependent {
            true => self.u.intern_type(Type::Refined(function, "apply".into(), info)),
            false => function,
        })
    }

    /// `t` at variance `v` with the references to the parameters of `ps` approximated by
    /// `Nothing .. their type` (dotty's `nonDependentResultApprox`); `None` where unsupported.
    fn approx(&mut self, t: TypeId, v: i8, ps: Params) -> Option<Approx> {
        let (binder, _, method) = ps;
        if !mentions_params(self.u, t, binder, Some(method)) {
            return Some(Approx::Exact(t));
        }
        if let Some(param) = self.param_type(t, ps) {
            return Some(approx::range(self.u, Approx::Exact(NOTHING), Approx::Exact(param), v));
        }
        match self.u.types().get(t).clone() {
            Type::Sym(Some(pre), member) => self.member_approx(pre, Ok(member), v, ps),
            Type::TypeRef(pre, name) => self.member_approx(pre, Err(name), v, ps),
            Type::Applied(tycon, args) => {
                let c = self.approx(tycon, v, ps)?;
                let variances = approx::tycon_variances(self.u, tycon, args.len());
                let mut mapped = Vec::with_capacity(args.len());
                for (&arg, &pv) in args.iter().zip(&variances) {
                    let wildcard = matches!(self.u.types().get(arg), Type::Bounds(..) | Type::Wildcard(_));
                    mapped.push(self.approx(arg, if wildcard { v } else { v * pv }, ps)?);
                }
                Some(approx::applied(self.u, c, &mapped, &variances, v))
            }
            Type::Refined(parent, name, info) => {
                let (parent, info) = (self.approx(parent, v, ps)?, self.approx(info, v, ps)?);
                Some(approx::refined(self.u, parent, &name, info, v))
            }
            Type::Bounds(lo, hi) => {
                let (lo, hi) = (self.approx(lo, -v, ps)?, self.approx(hi, v, ps)?);
                Some(approx::bounds(self.u, lo, hi, v))
            }
            Type::Alias(x) => {
                let x = self.approx(x, 0, ps)?;
                Some(approx::alias(self.u, x, v))
            }
            Type::And(x, y) | Type::Or(x, y, _) => {
                let parts = [self.approx(x, v, ps)?, self.approx(y, v, ps)?];
                let node = self.u.types().get(t).clone();
                Some(approx::derived(self.u, &parts, &[], v, &mut |u, p| match node {
                    Type::Or(_, _, soft) => u.intern_type(Type::Or(p[0], p[1], soft)),
                    _ => u.intern_type(Type::And(p[0], p[1])),
                }))
            }
            _ => None,
        }
    }

    /// The type of parameter reference `t` of `ps`.
    fn param_type(&mut self, t: TypeId, (binder, types, method): Params) -> Option<TypeId> {
        match *self.u.types().get(t) {
            Type::Param(b, i) if b == binder => types.get(i as usize).copied(),
            Type::Sym(None, p)
                if self.u.symbol(p).kind == Kind::Param && self.u.symbol(p).owner == Some(method) =>
            {
                Some(self.u.info(p))
            }
            _ => None,
        }
    }

    /// Member `member` of parameter reference `pre` of `ps`: a type by its alias or bounds, a
    /// term by `Nothing .. its type`.
    fn member_approx(
        &mut self,
        pre: TypeId,
        member: Result<SymbolId, String>,
        v: i8,
        ps: Params,
    ) -> Option<Approx> {
        let param = self.param_type(pre, ps)?;
        let member = match member {
            Ok(m) => m,
            Err(name) => {
                self.u.member_of_type(param, &name).into_iter().find(|&m| self.u.is_type_symbol(m))?
            }
        };
        let info = self.u.member_info(param, member);
        match *self.u.types().get(info) {
            Type::Alias(x) => Some(Approx::Exact(x)),
            Type::Bounds(lo, hi) => Some(approx::range(self.u, Approx::Exact(lo), Approx::Exact(hi), v)),
            _ if self.u.symbol(member).kind.is_term() => {
                Some(approx::range(self.u, Approx::Exact(NOTHING), Approx::Exact(info), v))
            }
            _ => None,
        }
    }
}
