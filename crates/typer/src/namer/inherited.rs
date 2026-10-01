//! The result type a class member without one inherits from the members it overrides (the
//! `inherited` of dotty's `Namer.inferredResultType`).

use crate::typer::Typer;
use syntax::ast::flags as f;
use tasty::{Kind, SymbolId, Type, TypeId};

impl Typer<'_> {
    /// The intersection of the result types of the members `sym` overrides, as seen from its class;
    /// `None` if it overrides none or one whose result depends on its parameters. A polymorphic
    /// method overriding anything is outside the subset.
    pub fn inherited_result(&mut self, sym: SymbolId) -> Option<TypeId> {
        let cls = self.u.symbol(sym).owner.filter(|&o| self.u.symbol(o).kind == Kind::Class)?;
        let name = self.u.symbol(sym).name.clone();
        let decls = self.u.symbol(sym).decls.clone();
        if decls.iter().any(|&d| self.u.symbol(d).kind == Kind::TypeParam) {
            if !self.class_members(cls, &name).iter().all(|&m| m == sym) {
                self.unsupported_at(syntax::ast::Span::NONE, "polymorphic method overriding a member");
            }
            return None;
        }
        let own_params = self.value_param_types(sym);
        let class = crate::defn::type_ref(self.u, cls);
        let site = self.u.intern_type(Type::This(class));
        let bases: Vec<SymbolId> = self.u.linearization(cls).into_iter().skip(1).collect();
        let mut result: Option<TypeId> = None;
        for base in bases {
            let members: Vec<SymbolId> = self.u.symbol(base).decls.clone();
            for m in members {
                let s = self.u.symbol(m);
                if s.name != name
                    || !s.kind.is_term()
                    || s.kind == Kind::Param
                    || s.flags.has(tasty::tags::PRIVATE)
                {
                    continue;
                }
                let Some(res) = self.matching_result(site, m, &own_params) else { continue };
                result = Some(match result {
                    Some(r) => self.u.glb(r, res),
                    None => res,
                });
            }
        }
        result
    }

    /// The parameter types of each value clause of script method `sym`, or none for a value.
    fn value_param_types(&mut self, sym: SymbolId) -> Vec<Vec<TypeId>> {
        let clauses = self.syms.param_clauses.get(&sym).cloned().unwrap_or_default();
        clauses.iter().map(|c| c.iter().map(|&p| self.u.info(p)).collect()).collect()
    }

    /// The result type of member `m` as seen from `site` if its value parameters have the types
    /// `params`, clause by clause.
    fn matching_result(&mut self, site: TypeId, m: SymbolId, params: &[Vec<TypeId>]) -> Option<TypeId> {
        self.complete(m);
        let mut t = self.u.member_info(site, m);
        for clause in params {
            let Type::Method { params: ps, result, .. } = self.u.types().get(t).clone() else { return None };
            if ps.len() != clause.len() {
                return None;
            }
            for (&(_, p), &own) in ps.iter().zip(clause) {
                if !(self.u.is_subtype(p, own) && self.u.is_subtype(own, p)) {
                    return None;
                }
            }
            t = result;
        }
        match self.u.types().get(t) {
            Type::Method { .. } | Type::Poly { .. } => None,
            Type::ByName(r) => Some(*r),
            _ => {
                let mut depends = false;
                self.u.map_type(t, &mut |u, x| {
                    depends |= matches!(u.types().get(x), Type::Param(..));
                    None
                });
                (!depends).then_some(t)
            }
        }
    }

    /// Whether `sym` keeps its right-hand side's type over an inherited one: an inline value.
    pub fn keeps_rhs_type(&self, sym: SymbolId) -> bool {
        let flags = self.syms.flags.get(&sym).copied().unwrap_or_default();
        flags.is(f::FINAL.or(f::INLINE)) && !flags.is(f::MUTABLE) && !flags.is(f::METHOD)
    }
}
