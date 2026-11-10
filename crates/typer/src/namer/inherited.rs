// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Namer.scala
//! The result type a class member without one inherits from the members it overrides (the
//! `inherited` of dotty's `Namer.inferredResultType`).

use crate::typer::Typer;
use syntax::ast::Tree;
use syntax::ast::flags as f;
use tasty::{Kind, NameId, SymbolId, Type, TypeId};

impl Typer<'_> {
    /// The intersection of the result types of the members `sym` overrides, as seen from its class,
    /// a polymorphic member's instantiated to the type parameters of `sym`; `None` if it overrides
    /// none or one whose result depends on its parameters.
    pub fn inherited_result(&mut self, sym: SymbolId) -> Option<TypeId> {
        let cls = self.u.symbol(sym).owner.filter(|&o| self.u.symbol(o).kind == Kind::Class)?;
        let name = self.u.symbol(sym).name.clone();
        let own_params = self.clauses(sym);
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
            let (p, s) = u.sym_ref(x)?;
            let owner = u.symbol(s).owner?;
            let (site, cls) = match p {
                None => (site, cls),
                Some(_) if p == Some(site) => (site, cls),
                Some(pre) => {
                    let c = u.this_of(pre)?;
                    (pre, u.class_symbol(c)?)
                }
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

    /// The parameter clauses of script method `sym`, each with whether it is a type clause, an
    /// empty clause `()` among them, or none for a value; one value clause per clause it recorded
    /// for a method without a definition.
    fn clauses(&mut self, sym: SymbolId) -> Vec<(bool, Vec<SymbolId>)> {
        let clauses = self.syms.param_clauses.get(&sym).cloned().unwrap_or_default();
        let Some(&tree) = self.syms.def_trees.get(&sym) else {
            return clauses.into_iter().map(|c| (false, c)).collect();
        };
        let decls = self.u.symbol(sym).decls.clone();
        let mut types = decls.into_iter().filter(|&d| self.u.symbol(d).kind == Kind::TypeParam);
        let mut values = clauses.into_iter();
        let mut out = Vec::new();
        for clause in self.def_clauses(tree) {
            match clause.first() {
                None => out.push((false, Vec::new())),
                Some(&p) if matches!(self.ast.tree(p), Tree::TypeDef { .. }) => {
                    out.push((true, types.by_ref().take(clause.len()).collect()));
                }
                Some(_) => out.push((false, values.next().unwrap_or_default())),
            }
        }
        out
    }

    /// The result type of member `m` as seen from `site` if its clauses take as many type
    /// parameters as those of `clauses`, instantiated to them, and value parameters of their types,
    /// `Any` matching a Java method's `Object`; its parameters replaced by those of `clauses`.
    fn matching_result(
        &mut self,
        site: TypeId,
        m: SymbolId,
        clauses: &[(bool, Vec<SymbolId>)],
    ) -> Option<TypeId> {
        self.complete(m);
        let java = self.is_java_member(m);
        let mut t = self.u.member_info(site, m);
        t = self.u.map_type(t, &mut |u, x| Some(u.beta_reduced(x)).filter(|&r| r != x));
        let decls = self.u.symbol(m).decls.clone();
        let their_types: Vec<SymbolId> =
            decls.into_iter().filter(|&d| self.u.symbol(d).kind == Kind::TypeParam).collect();
        let their_values: Vec<SymbolId> =
            self.clauses(m).into_iter().filter(|c| !c.0).flat_map(|c| c.1).collect();
        let (mut targs, mut refs) = (Vec::new(), Vec::new());
        for (types, clause) in clauses {
            if *types {
                let (binder, ps, result) = self.u.poly(t).map(|b| (b.binder, b.params.to_vec(), b.result))?;
                if ps.len() != clause.len() {
                    return None;
                }
                let start = targs.len();
                targs.extend(clause.iter().map(|&p| self.u.intern_type(Type::Sym(None, p))));
                let names: Vec<NameId> = ps.into_iter().map(|(n, _)| n).collect();
                t = self.u.subst(result, binder, &names, &targs[start..]);
                let n = targs.len().min(their_types.len());
                t = self.subst_syms(t, &their_types[..n], &targs[..n]);
                t = self.u.map_type(t, &mut |u, x| Some(u.beta_reduced(x)).filter(|&r| r != x));
                continue;
            }
            let (binder, ps, result) = self.u.method(t).map(|b| (b.binder, b.params.to_vec(), b.result))?;
            if ps.len() != clause.len() {
                return None;
            }
            for (&(_, p), &own) in ps.iter().zip(clause) {
                let own = self.u.info(own);
                let same = self.u.is_subtype(p, own) && self.u.is_subtype(own, p);
                let object_any = java
                    && self.u.is_class_named(p, "java.lang.Object")
                    && self.u.is_class_named(own, "scala.Any");
                if !same && !object_any {
                    return None;
                }
            }
            let names: Vec<NameId> = ps.into_iter().map(|(n, _)| n).collect();
            let start = refs.len();
            refs.extend(clause.iter().map(|&p| self.ref_type(None, p)));
            t = self.u.subst(result, binder, &names, &refs[start..]);
        }
        t = self.subst_syms(t, &their_types, &targs);
        t = self.subst_syms(t, &their_values, &refs);
        match *self.u.ty(t) {
            Type::Method { .. } | Type::Poly { .. } => None,
            Type::ByName(r) => Some(r),
            _ => {
                let mut depends = false;
                self.u.map_type(t, &mut |u, x| {
                    depends |= u.param_ref(x).is_some();
                    None
                });
                (!depends).then_some(t)
            }
        }
    }

    /// `t` with references to `from` replaced by `to` if both have the same length.
    fn subst_syms(&mut self, t: TypeId, from: &[SymbolId], to: &[TypeId]) -> TypeId {
        if from.len() != to.len() {
            return t;
        }
        self.u.map_type(t, &mut |u, x| match u.sym_ref(x) {
            Some((None, s)) => from.iter().position(|&p| p == s).map(|i| to[i]),
            _ => None,
        })
    }

    /// Whether `sym` keeps its right-hand side's type over an inherited one: an inline value.
    pub fn keeps_rhs_type(&self, sym: SymbolId) -> bool {
        let flags = self.syms.flags.get(&sym).copied().unwrap_or_default();
        flags.is(f::FINAL.or(f::INLINE)) && !flags.is(f::MUTABLE) && !flags.is(f::METHOD)
    }
}
