// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/backend/jvm/GenericSignatures.scala
//! Signatures of method types (dotty's `jsig` of `MethodOrPoly`).

use super::{Boxing, Sig};
use names::nme;
use tasty::{Type, TypeId};

impl Sig<'_> {
    /// A method type: type parameters at the top, of an extension method then its value
    /// class's, parameters, result.
    pub fn method(&mut self, mut t: TypeId, top: bool) {
        let mut tparams = Vec::new();
        let mut vparams = Vec::new();
        loop {
            match *self.u.ty(t) {
                Type::Poly { binder, params, result } => {
                    let encoded: Vec<(String, TypeId)> = self
                        .u
                        .params_of(params)
                        .iter()
                        .map(|&(n, b)| (tasty::encode(self.u.name(n)), b))
                        .collect();
                    if top && !self.constructor {
                        tparams.extend(encoded.iter().cloned());
                    }
                    self.poly.push((binder, encoded));
                    t = result;
                }
                Type::Method { params, result, .. } => {
                    let params = self.u.params_of(params);
                    let names: Vec<&str> = params.iter().map(|&(n, _)| self.u.name(n)).collect();
                    let java_enum =
                        self.constructor && names.ends_with(&[nme::NAME_DOLLAR_, nme::ORDINAL_DOLLAR_]);
                    let keep = params.len() - if java_enum { 2 } else { 0 };
                    let outer = usize::from(self.constructor && names.first() == Some(&nme::OUTER));
                    vparams.extend(params.iter().take(keep).skip(outer).map(|&(_, p)| p));
                    t = self.context_result(result, &mut vparams);
                }
                _ => break,
            }
        }
        vparams.retain(|&p| !self.u.is_erased_param(p));
        if let Some(c) = self.extended.filter(|_| top) {
            let class_params = self.u.type_params(c).into_iter();
            let class_params = class_params.map(|p| (tasty::encode(&self.u.symbol(p).name), self.u.info(p)));
            tparams.extend(class_params.collect::<Vec<_>>());
        }
        self.type_params(&tparams);
        self.out.push('(');
        for p in vparams {
            self.jsig(p, false, Boxing::Unbox);
        }
        self.out.push(')');
        self.result(t);
    }

    /// The result type of context function type `t` with its parameter types added to `vparams`,
    /// else `t` (dotty's `collectMethodParams`).
    fn context_result(&mut self, t: TypeId, vparams: &mut Vec<TypeId>) -> TypeId {
        let d = self.u.dealias(t);
        let Some((tycon, args)) = self.u.applied(d) else { return t };
        let mut args = args.to_vec();
        let name = self.u.class_symbol(tycon).map(|c| self.u.symbol(c).name.clone());
        let Some(arity) = name.as_deref().and_then(|n| n.strip_prefix("ContextFunction")) else { return t };
        if arity.is_empty() || !arity.bytes().all(|b| b.is_ascii_digit()) {
            return t;
        }
        let Some(result) = args.pop() else { return t };
        vparams.extend(args);
        result
    }
}
