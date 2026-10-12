//! The types `TypeAssigner` gives applications and type applications.

use super::{Refusal, Unpickler};
use crate::defn::type_ref;
use crate::tpd::TNodeId;
use crate::typer::dependent_params::applied_result;
use crate::typer::seen_from::normalized;
use crate::typer::type_apply::instantiate_poly;
use tasty::{Kind, SymbolId, Type, TypeId};

impl Unpickler<'_> {
    /// The class of full name `full`.
    pub fn class_symbol(&mut self, full: &str) -> Result<SymbolId, Refusal> {
        let r = self.u.class_ref(full);
        self.u.class_symbol(r).ok_or("a class missing from the library")
    }

    /// The class of full name `full`, referred to from where it is visible.
    pub fn class_type(&mut self, full: &str) -> Result<TypeId, Refusal> {
        let c = self.class_symbol(full)?;
        Ok(type_ref(self.u, c))
    }

    /// The type `fun` applies, seen from its prefix, and the method it refers to.
    fn method_type(&mut self, fun: TNodeId) -> (TypeId, Option<SymbolId>) {
        let tpe = self.tpd.tpe(fun);
        let meth = self.tpd.sym(self.tpd.meth_part(fun));
        match *self.u.types().get(tpe) {
            Type::Sym(p, s) if self.u.symbol(s).kind == Kind::Def => {
                let info = match p.filter(|&p| !matches!(self.u.types().get(p), Type::Package(_))) {
                    Some(p) => self.u.member_info(p, s),
                    None => self.u.info(s),
                };
                (normalized(self.u, info), Some(s))
            }
            _ => (tpe, meth),
        }
    }

    /// The type of `fun(args)`: the result of `fun`'s method type, its parameters replaced by the
    /// arguments' types.
    pub fn apply_type(&mut self, fun: TNodeId, args: &[TNodeId]) -> Result<TypeId, Refusal> {
        if self.tpd.sym(fun).is_some_and(|s| self.u.symbol(s).name == "<init>") {
            return Err("constructor call in a library inline body");
        }
        let (info, meth) = self.method_type(fun);
        let Type::Method { binder, params, result, .. } = self.u.types().get(info).clone() else {
            return Err("application of a value in a library inline body");
        };
        if params.len() != args.len() {
            return Err("application with missing arguments in a library inline body");
        }
        let names: Vec<String> = params.into_iter().map(|(n, _)| n).collect();
        let args: Vec<TypeId> = args.iter().map(|&a| self.tpd.tpe(a)).collect();
        applied_result(self.u, result, (binder, &names), meth, &args)
            .map_err(|_| "dependent result of an unstable argument in a library inline body")
    }

    /// The type of `fun[args]`: the result of `fun`'s polymorphic type instantiated.
    pub fn type_apply_type(&mut self, fun: TNodeId, args: &[TNodeId]) -> Result<TypeId, Refusal> {
        let (info, meth) = self.method_type(fun);
        let Type::Poly { binder, params, result } = self.u.types().get(info).clone() else {
            return Err("type application of a value in a library inline body");
        };
        if params.len() != args.len() {
            return Err("type application with missing arguments in a library inline body");
        }
        let names: Vec<String> = params.into_iter().map(|(n, _)| n).collect();
        let args: Vec<TypeId> = args.iter().map(|&a| self.tpd.tpe(a)).collect();
        Ok(instantiate_poly(self.u, result, (binder, &names), &args, meth))
    }
}
