// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Namer.scala
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
        let own_params = self.value_params(sym);
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
                self.complete_type_members(res);
                result = Some(match result {
                    Some(r) => self.u.glb(r, res),
                    None => res,
                });
            }
        }
        result
    }

    /// `t` with each type member of a base class of `cls` selected from `site`, `cls`'s `this`,
    /// or of a base class of an enclosing class selected from its `this`, replaced by the member
    /// of that name that class sees.
    fn rebound(&mut self, t: TypeId, site: TypeId, cls: SymbolId) -> TypeId {
        self.u.map_type(t, &mut |u, x| {
            let &Type::Sym(p, s) = u.types().get(x) else { return None };
            let owner = u.symbol(s).owner?;
            let (site, cls) = match p.map(|p| u.types().get(p).clone()) {
                None => (site, cls),
                Some(_) if p == Some(site) => (site, cls),
                Some(Type::This(c)) => (p?, u.class_symbol(c)?),
                Some(_) => return None,
            };
            let bases = u.linearization(cls);
            if u.symbol(s).kind != Kind::Type || !bases.contains(&owner) {
                return None;
            }
            let name = u.symbol(s).name.clone();
            let visible = |d: &SymbolId| {
                let d = u.symbol(*d);
                matches!(d.kind, Kind::Type | Kind::Class)
                    && d.name == name
                    && !d.flags.has(tasty::tags::PRIVATE)
            };
            let m = bases.iter().find_map(|&b| u.symbol(b).decls.iter().copied().find(visible))?;
            if m == s {
                return None;
            }
            Some(match u.symbol(m).kind {
                Kind::Class => crate::defn::type_ref(u, m),
                _ => u.intern_type(Type::Sym(Some(site), m)),
            })
        })
    }

    /// The parameters of each value clause of script method `sym`, an empty clause `()` among
    /// them, or none for a value.
    fn value_params(&mut self, sym: SymbolId) -> Vec<Vec<SymbolId>> {
        let clauses = self.syms.param_clauses.get(&sym).cloned().unwrap_or_default();
        let Some(&tree) = self.syms.def_trees.get(&sym) else { return clauses };
        let mut values = clauses.into_iter();
        let mut out = Vec::new();
        for clause in self.def_clauses(tree) {
            match clause.first() {
                None => out.push(Vec::new()),
                Some(&p) if matches!(self.ast.tree(p), Tree::TypeDef { .. }) => {}
                Some(_) => {
                    out.push(values.next().unwrap_or_default());
                }
            }
        }
        out
    }

    /// The result type of member `m` as seen from `site` if it has as many type parameters as
    /// `tparams`, instantiated to them, and its value parameters have the types of `params`, clause
    /// by clause, `Any` matching a Java method's `Object`; its parameters replaced by `params`.
    fn matching_result(
        &mut self,
        site: TypeId,
        m: SymbolId,
        tparams: &[SymbolId],
        params: &[Vec<SymbolId>],
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
        let theirs: Vec<SymbolId> = self.value_params(m).concat();
        let mut refs = Vec::new();
        for clause in params {
            let Type::Method { binder, params: ps, result, .. } = self.u.types().get(t).clone() else {
                return None;
            };
            if ps.len() != clause.len() {
                return None;
            }
            for (&(_, p), &own) in ps.iter().zip(clause) {
                let own = self.u.info(own);
                let same = self.u.is_subtype(p, own) && self.u.is_subtype(own, p);
                let object_any =
                    java && self.is_class(p, "java.lang.Object") && self.is_class(own, "scala.Any");
                if !same && !object_any {
                    return None;
                }
            }
            let names: Vec<String> = ps.into_iter().map(|(n, _)| n).collect();
            let start = refs.len();
            refs.extend(clause.iter().map(|&p| self.ref_type(None, p)));
            t = self.u.subst(result, binder, &names, &refs[start..]);
        }
        if theirs.len() == refs.len() {
            t = self.u.map_type(t, &mut |u, x| match *u.types().get(x) {
                Type::Sym(None, s) => theirs.iter().position(|&p| p == s).map(|i| refs[i]),
                _ => None,
            });
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
