// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/RefChecks.scala
//   compiler/src/dotty/tools/dotc/core/SymDenotations.scala
//! Concrete classes with abstract members left unimplemented, which dotty's
//! `checkNoAbstractMembers` rejects, made to fall back.

use super::Typer;
use rustc_hash::FxHashMap;
use syntax::ast::Span;
use tasty::tags::{ABSTRACT, CASE, MUTABLE, PRIVATE, TRAIT};
use tasty::{Kind, SymbolId, Type, TypeId};

impl Typer<'_> {
    /// Refuses class `cls` if it is concrete and an abstract term member of it or of a base class
    /// has no concrete member of `cls` matching it.
    pub fn check_no_abstract_members(&mut self, cls: SymbolId, site: TypeId, span: Span) {
        if self.u.symbol(cls).flags.has(ABSTRACT) || self.u.symbol(cls).flags.has(TRAIT) {
            return;
        }
        let bases = self.u.linearization(cls);
        let case = bases.iter().any(|&b| self.u.symbol(b).flags.has(CASE));
        let site = self.u.intern_type(Type::This(site));
        for base in bases {
            let synthesized = match self.u.full_name(base).as_str() {
                "scala.Product" | "scala.Equals" => case,
                "scala.reflect.Enum" => true,
                _ => false,
            };
            if synthesized {
                continue;
            }
            let decls = self.u.symbol(base).decls.clone();
            for d in decls {
                let (kind, name) = (self.u.symbol(d).kind, self.u.symbol(d).name.clone());
                if !matches!(kind, Kind::Val | Kind::Def)
                    || name == "<init>"
                    || name.contains("$$super$")
                    || name.contains("$_setter_$")
                    || self.is_concrete(d)
                    || self.is_var_setter(base, d)
                {
                    continue;
                }
                let members = self.members_named(cls, &name, &mut FxHashMap::default());
                self.complete(d);
                let implemented = members.into_iter().any(|m| {
                    self.complete(m);
                    self.is_concrete(m) && (self.same_member(site, m, d) || self.same_shape(m, d))
                });
                if !implemented {
                    return self.unsupported_at(span, "concrete class with an unimplemented member");
                }
            }
        }
    }

    /// Whether methods `a` and `b` take as many type and value parameters in each clause, as
    /// `matches` does not compare interleaved clauses and path-dependent parameter types.
    fn same_shape(&mut self, a: SymbolId, b: SymbolId) -> bool {
        let def = |t: &Self, s| t.u.symbol(s).kind == Kind::Def;
        def(self, a) && def(self, b) && self.clause_shape(a) == self.clause_shape(b)
    }

    fn clause_shape(&mut self, s: SymbolId) -> Vec<(bool, usize)> {
        let mut t = self.u.info(s);
        let mut out = Vec::new();
        loop {
            match self.u.types().get(t) {
                Type::Poly { params, result, .. } => (out.push((true, params.len())), t = *result),
                Type::Method { params, result, .. } => (out.push((false, params.len())), t = *result),
                _ => return out,
            };
        }
    }

    /// Whether `d`, a declaration of class `c`, is the setter of a variable of `c`; the variable
    /// is checked itself.
    fn is_var_setter(&mut self, c: SymbolId, d: SymbolId) -> bool {
        let Some(getter) = self.u.symbol(d).name.strip_suffix("_=").map(str::to_owned) else { return false };
        let decls = self.u.symbol(c).decls.clone();
        decls.into_iter().any(|v| {
            let s = self.u.symbol(v);
            s.name == getter && matches!(s.kind, Kind::Val | Kind::Param) && s.flags.has(MUTABLE)
        })
    }

    /// The non-private term members of class `c` named `name`: its own, and those of its parents
    /// that no own member matches (dotty's `nonPrivateMembersNamed`).
    fn members_named(
        &mut self,
        c: SymbolId,
        name: &str,
        memo: &mut FxHashMap<SymbolId, Vec<SymbolId>>,
    ) -> Vec<SymbolId> {
        if let Some(found) = memo.get(&c) {
            return found.clone();
        }
        memo.insert(c, Vec::new());
        let own: Vec<SymbolId> = self.u.symbol(c).decls.clone();
        let own: Vec<SymbolId> = own
            .into_iter()
            .filter(|&d| {
                let s = self.u.symbol(d);
                s.name == name && s.kind.is_term() && !s.flags.has(PRIVATE)
            })
            .collect();
        own.iter().for_each(|&o| self.complete(o));
        let site = crate::defn::type_ref(self.u, c);
        let mut out = own.clone();
        for p in self.u.parents(c) {
            let Some(pc) = self.u.class_symbol(p) else { continue };
            for m in self.members_named(pc, name, memo) {
                if !out.contains(&m) && !own.iter().any(|&o| self.same_member(site, o, m)) {
                    out.push(m);
                }
            }
        }
        memo.insert(c, out.clone());
        out
    }
}
