//! The result type a class member without one inherits from the members it overrides (the
//! `inherited` of dotty's `Namer.inferredResultType`).

use crate::symbols::Completion;
use crate::typer::Typer;
use syntax::ast::Tree;
use syntax::ast::flags as f;
use tasty::{Kind, SymbolId, Type, TypeId};

impl Typer<'_> {
    /// The intersection of the result types of the members `sym` overrides, as seen from its class,
    /// a polymorphic member's instantiated to the type parameters of `sym`; `None` if it overrides
    /// none or one whose result depends on its parameters.
    pub fn inherited_result(&mut self, sym: SymbolId) -> Option<TypeId> {
        let cls = self.u.symbol(sym).owner.filter(|&o| self.u.symbol(o).kind == Kind::Class)?;
        let name = self.u.symbol(sym).name.clone();
        let decls = self.u.symbol(sym).decls.clone();
        let tparams: Vec<SymbolId> =
            decls.into_iter().filter(|&d| self.u.symbol(d).kind == Kind::TypeParam).collect();
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
                let Some(res) = self.matching_result(site, m, &tparams, &own_params) else { continue };
                let res = self.rebound(res, site, cls);
                result = Some(match result {
                    Some(r) => self.u.glb(r, res),
                    None => res,
                });
            }
        }
        result
    }

    /// `t` with each type member of a base class of `cls` selected from `site`, `cls`'s `this`,
    /// replaced by the member of that name `cls` sees.
    fn rebound(&mut self, t: TypeId, site: TypeId, cls: SymbolId) -> TypeId {
        let bases = self.u.linearization(cls);
        self.u.map_type(t, &mut |u, x| {
            let &Type::Sym(p, s) = u.types().get(x) else { return None };
            let owner = u.symbol(s).owner?;
            if p.is_some_and(|p| p != site) || u.symbol(s).kind != Kind::Type || !bases.contains(&owner) {
                return None;
            }
            let name = u.symbol(s).name.clone();
            let visible = |d: &SymbolId| {
                let d = u.symbol(*d);
                d.kind == Kind::Type && d.name == name && !d.flags.has(tasty::tags::PRIVATE)
            };
            let m = bases.iter().find_map(|&b| u.symbol(b).decls.iter().copied().find(visible))?;
            (m != s).then(|| u.intern_type(Type::Sym(Some(site), m)))
        })
    }

    /// The parameter types of each value clause of script method `sym`, or none for a value.
    fn value_param_types(&mut self, sym: SymbolId) -> Vec<Vec<TypeId>> {
        let clauses = self.syms.param_clauses.get(&sym).cloned().unwrap_or_default();
        clauses.iter().map(|c| c.iter().map(|&p| self.u.info(p)).collect()).collect()
    }

    /// The result type of member `m` as seen from `site` if it has as many type parameters as
    /// `tparams`, instantiated to them, and its value parameters have the types `params`, clause
    /// by clause, `Any` matching a Java method's `Object`.
    fn matching_result(
        &mut self,
        site: TypeId,
        m: SymbolId,
        tparams: &[SymbolId],
        params: &[Vec<TypeId>],
    ) -> Option<TypeId> {
        self.complete(m);
        let java = self.is_java_member(m);
        let mut t = self.u.member_info(site, m);
        if !tparams.is_empty() {
            let Type::Poly { binder, params: ps, result } = self.u.types().get(t).clone() else {
                return None;
            };
            if ps.len() != tparams.len() {
                return None;
            }
            let args: Vec<TypeId> = tparams.iter().map(|&p| self.u.intern_type(Type::Sym(None, p))).collect();
            let names: Vec<String> = ps.into_iter().map(|(n, _)| n).collect();
            t = self.u.subst(result, binder, &names, &args);
        }
        t = self.u.map_type(t, &mut |u, x| Some(u.beta_reduced(x)).filter(|&r| r != x));
        for clause in params {
            let Type::Method { params: ps, result, .. } = self.u.types().get(t).clone() else { return None };
            if ps.len() != clause.len() {
                return None;
            }
            for (&(_, p), &own) in ps.iter().zip(clause) {
                let same = self.u.is_subtype(p, own) && self.u.is_subtype(own, p);
                let object_any =
                    java && self.is_class(p, "java.lang.Object") && self.is_class(own, "scala.Any");
                if !same && !object_any {
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

    /// Whether `t` is a reference to the class named `name`.
    fn is_class(&mut self, t: TypeId, name: &str) -> bool {
        self.u.class_symbol(t).is_some_and(|c| self.u.full_name(c) == name)
    }

    /// Reports implicit method `sym`, or implicit value of a class, whose type is inferred, and
    /// makes it a plain one (dotty's `missingType`).
    pub fn check_inferred_implicit(&mut self, sym: SymbolId) {
        if !self.u.symbol(sym).flags.has(tasty::tags::IMPLICIT) {
            return;
        }
        let Some(&Completion::Completing(tree)) = self.syms.completions.get(&sym) else { return };
        let member = self.u.symbol(sym).owner.is_some_and(|o| self.u.symbol(o).kind == Kind::Class);
        let modifier = match self.ast.tree(tree) {
            Tree::DefDef { .. } => "result ",
            Tree::ValDef { .. } if member => "",
            _ => return,
        };
        let span = self.ast.span(tree);
        let msg = format!("{modifier}type of implicit definition needs to be given explicitly");
        self.diags.error_at(span.start(), span.end(), span.point(), msg);
        let flags = self.u.symbol(sym).flags.without(tasty::tags::IMPLICIT);
        self.u.set_flags(sym, flags);
    }

    /// Whether `sym` keeps its right-hand side's type over an inherited one: an inline value.
    pub fn keeps_rhs_type(&self, sym: SymbolId) -> bool {
        let flags = self.syms.flags.get(&sym).copied().unwrap_or_default();
        flags.is(f::FINAL.or(f::INLINE)) && !flags.is(f::MUTABLE) && !flags.is(f::METHOD)
    }
}
