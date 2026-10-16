//! The infos and parameters of export forwarders.

use crate::typer::Typer;
use syntax::ast::flags as f;
use tasty::tags as t;
use tasty::{Kind, SymbolId, Type, TypeId};

impl Typer<'_> {
    /// Gives forwarder `fwd` to term member `m` of `path` the info of `m` seen from `path`, with
    /// parameters of its own (dotty's `ensureMethodic`); for an export in an extension, the
    /// clauses of its path method `path_method` first (dotty's `addPathMethodParams`).
    pub fn set_forwarder_info(
        &mut self,
        fwd: SymbolId,
        m: SymbolId,
        path: TypeId,
        path_method: Option<SymbolId>,
    ) {
        self.complete(m);
        let mut clauses: Vec<Vec<SymbolId>> = Vec::new();
        let mut pairs: Vec<(SymbolId, SymbolId)> = Vec::new();
        let path = match path_method {
            Some(pm) => {
                let info = self.u.info(pm);
                self.forwarder_clauses(fwd, pm, info, &mut clauses, &mut pairs)
            }
            None => path,
        };
        let info = self.u.member_info(path, m);
        let info = self.resolve_class_params(info);
        let info = match *self.u.types().get(info) {
            Type::ByName(r) => r,
            _ => info,
        };
        let tpe = self.forwarder_clauses(fwd, m, info, &mut clauses, &mut pairs);
        let renamed: Vec<SymbolId> = clauses.iter().flatten().copied().collect();
        for &s in &renamed {
            let info = self.u.info(s);
            let info = self.with_params_renamed(info, &pairs);
            self.u.set_info(s, info);
            if let Some(&r) = self.syms.repeated_params.get(&s) {
                let r = self.with_params_renamed(r, &pairs);
                self.syms.repeated_params.insert(s, r);
            }
        }
        let result = self.with_params_renamed(tpe, &pairs);
        self.set_forwarder_result(fwd, clauses, result);
    }

    /// Records parameter clauses `clauses` and result type `result` of forwarder `fwd` and sets
    /// its info from them.
    pub fn set_forwarder_result(&mut self, fwd: SymbolId, clauses: Vec<Vec<SymbolId>>, result: TypeId) {
        let values = clauses.iter().filter(|c| c.first().is_none_or(|&p| !self.u.is_type_symbol(p))).cloned();
        self.syms.param_clauses.insert(fwd, values.collect());
        self.syms.def_results.insert(fwd, result);
        self.syms.declared_results.insert(fwd);
        let info = self.clauses_info(clauses, result);
        self.u.set_info(fwd, info);
    }

    /// Pushes onto `clauses` parameters of forwarder `fwd` for the clauses of `info`, the info of
    /// method `src`, and onto `pairs` each parameter of `src` with its copy; the result type
    /// after them.
    fn forwarder_clauses(
        &mut self,
        fwd: SymbolId,
        src: SymbolId,
        mut info: TypeId,
        clauses: &mut Vec<Vec<SymbolId>>,
        pairs: &mut Vec<(SymbolId, SymbolId)>,
    ) -> TypeId {
        let originals: Vec<SymbolId> = self.u.decls(src).unwrap_or_default();
        let of_kind =
            |t: &Self, k: Kind| originals.iter().copied().filter(|&p| t.u.symbol(p).kind == k).collect();
        let (mut types, mut terms): (Vec<SymbolId>, Vec<SymbolId>) =
            (of_kind(self, Kind::TypeParam), of_kind(self, Kind::Param));
        types.reverse();
        terms.reverse();
        let taken: Vec<String> = clauses.iter().flatten().map(|&p| self.u.symbol(p).name.clone()).collect();
        let fresh = |n: &str| if taken.iter().any(|t| t == n) { format!("{n}$1") } else { n.to_owned() };
        loop {
            let (binder, params, result, poly, flags) = match self.u.types().get(info).clone() {
                Type::Poly { binder, params, result } => (binder, params, result, true, f::TYPE_PARAM),
                Type::Method { binder, params, result, implicit, contextual } => {
                    let flags = match (contextual, implicit) {
                        (true, _) => f::TERM_PARAM.or(f::GIVEN),
                        (false, true) => f::TERM_PARAM.or(f::IMPLICIT),
                        _ => f::TERM_PARAM,
                    };
                    (binder, params, result, false, flags)
                }
                _ => return info,
            };
            let mut syms = Vec::new();
            for (n, p) in &params {
                let original = if poly { types.pop() } else { terms.pop() };
                let default = original.is_some_and(|o| self.u.symbol(o).flags.has(t::HASDEFAULT));
                let flags = if default { flags.or(f::HAS_DEFAULT) } else { flags };
                let kind = if poly { Kind::TypeParam } else { Kind::Param };
                let s = self.syms.define(self.u, &fresh(n), kind, fwd, flags);
                pairs.extend(original.map(|o| (o, s)));
                syms.push((s, *p));
            }
            let names: Vec<String> = params.into_iter().map(|(n, _)| n).collect();
            let args: Vec<TypeId> =
                syms.iter().map(|&(s, _)| self.u.intern_type(Type::Sym(None, s))).collect();
            for &(s, p) in &syms {
                let p = self.u.subst(p, binder, &names, &args);
                match poly {
                    true => self.u.set_info(s, p),
                    false => self.set_param_info(s, p),
                }
            }
            info = self.u.subst(result, binder, &names, &args);
            clauses.push(syms.into_iter().map(|(s, _)| s).collect());
        }
    }

    /// Sets the info of forwarder parameter `s` of type `p` in its method's type: `Seq[T]` for a
    /// repeated one, which is recorded as such.
    fn set_param_info(&mut self, s: SymbolId, p: TypeId) {
        let Some(elem) = self.repeated_elem(p) else { return self.u.set_info(s, p) };
        let seq = self.resolve_full_name("scala.collection.immutable.Seq", true);
        let annot = self.resolve_full_name("scala.annotation.internal.Repeated", true);
        let (Some(seq), Some(annot)) = (seq, annot) else { return self.u.set_info(s, p) };
        let seq = crate::defn::type_ref(self.u, seq);
        let seq = self.u.intern_type(Type::Applied(seq, vec![elem]));
        let annot = crate::defn::type_ref(self.u, annot);
        let written = self.u.intern_type(Type::Annotated(seq, annot));
        self.syms.repeated_params.insert(s, written);
        self.u.set_info(s, seq);
    }

    /// `t` with references to the parameters `pairs` map made references to their images.
    fn with_params_renamed(&mut self, t: TypeId, pairs: &[(SymbolId, SymbolId)]) -> TypeId {
        if pairs.is_empty() {
            return t;
        }
        self.u.map_type(t, &mut |u, x| match *u.types().get(x) {
            Type::Sym(None, s) => {
                pairs.iter().find(|&&(o, _)| o == s).map(|&(_, r)| u.intern_type(Type::Sym(None, r)))
            }
            _ => None,
        })
    }
}
