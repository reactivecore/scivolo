//! What dotty's `RefChecks.checkAllOverrides` rejects and the typer does not port yet, made to fall
//! back: a member overriding a concrete one without `override`, and concrete members of traits
//! inherited in conflict.

use super::Typer;
use syntax::ast::Span;
use syntax::ast::flags as f;
use tasty::{Kind, SymbolId, Type, TypeId};

impl Typer<'_> {
    pub fn is_concrete(&self, s: SymbolId) -> bool {
        match self.syms.flags.get(&s) {
            Some(flags) => !flags.is(f::DEFERRED),
            None => self.u.body(s).is_some(),
        }
    }

    fn has_default_args(&self, def: SymbolId) -> bool {
        let clauses = self.syms.param_clauses.get(&def);
        clauses
            .into_iter()
            .flatten()
            .flatten()
            .any(|p| self.syms.flags.get(p).is_some_and(|fl| fl.is(f::HAS_DEFAULT)))
    }

    /// The term members of class `c` named `name` that a subclass can override.
    fn overridable(&self, c: SymbolId, name: &str) -> Vec<SymbolId> {
        let decls = &self.u.symbol(c).decls;
        let keep = |&&d: &&SymbolId| {
            let s = self.u.symbol(d);
            s.name == name && s.kind.is_term() && s.kind != Kind::Param && !s.flags.has(tasty::tags::PRIVATE)
        };
        decls.iter().filter(keep).copied().collect()
    }

    pub fn same_member(&mut self, site: TypeId, a: SymbolId, b: SymbolId) -> bool {
        let (ka, kb) = (self.u.symbol(a).kind, self.u.symbol(b).kind);
        if ka == Kind::Def && kb == Kind::Def { self.u.matches(site, a, b) } else { true }
    }

    pub fn check_overrides(&mut self, cls: SymbolId, span: Span) {
        let site = crate::defn::type_ref(self.u, cls);
        let bases: Vec<SymbolId> = self.u.linearization(cls).into_iter().skip(1).collect();
        let own: Vec<SymbolId> = self.u.symbol(cls).decls.clone();
        let defs: Vec<SymbolId> =
            own.iter().copied().filter(|&d| self.u.symbol(d).kind == Kind::Def).collect();
        for (k, &a) in defs.iter().enumerate() {
            for &b in &defs[..k] {
                if self.u.symbol(a).name != self.u.symbol(b).name {
                    continue;
                }
                let type_params = |t: &mut Self, m| {
                    let info = t.u.info(m);
                    match t.u.types().get(info) {
                        Type::Poly { params, .. } => params.len(),
                        _ => 0,
                    }
                };
                let same_arity = type_params(self, a) == type_params(self, b);
                let erased_clash = same_arity && self.u.erased_method(a) == self.u.erased_method(b);
                if self.u.matches(site, a, b) || erased_clash {
                    return self.unsupported_at(span, "double definition");
                }
                if self.has_default_args(a) && self.has_default_args(b) {
                    return self.unsupported_at(span, "overloads with default arguments");
                }
            }
        }
        for d in own {
            let (name, kind) = (self.u.symbol(d).name.clone(), self.u.symbol(d).kind);
            let exempt = f::PRIVATE.or(f::SYNTHETIC);
            let getter = name.contains("$default$");
            let flags = self.syms.flags.get(&d).copied();
            if !kind.is_term()
                || kind == Kind::Param
                || name == "<init>"
                || getter
                || flags.is_none_or(|fl| fl.is_one_of(exempt))
            {
                continue;
            }
            let flags = flags.unwrap_or_default();
            for &b in &bases {
                for m in self.overridable(b, &name) {
                    if !self.is_concrete(m) || !self.same_member(site, d, m) {
                        continue;
                    }
                    if flags.is(f::DEFERRED) {
                        return self.unsupported_at(span, "abstract member overriding a concrete one");
                    }
                    if !flags.is(f::OVERRIDE) {
                        return self.unsupported_at(span, "override without override modifier");
                    }
                }
            }
        }
        let own_names: Vec<String> =
            self.u.symbol(cls).decls.iter().map(|&d| self.u.symbol(d).name.clone()).collect();
        let traits: Vec<SymbolId> =
            bases.iter().copied().filter(|&b| self.u.symbol(b).flags.has(tasty::tags::TRAIT)).collect();
        for &t in &traits {
            let names: Vec<String> =
                self.u.symbol(t).decls.iter().map(|&d| self.u.symbol(d).name.clone()).collect();
            for name in names.iter().filter(|n| !own_names.contains(n) && n.as_str() != "<init>") {
                let concrete: Vec<SymbolId> = bases
                    .iter()
                    .flat_map(|&b| self.overridable(b, name))
                    .filter(|&m| self.is_concrete(m))
                    .collect();
                let Some((&winner, rest)) = concrete.split_first() else { continue };
                let Some(winner_owner) = self.u.symbol(winner).owner else { continue };
                for &m in rest {
                    let owner = self.u.symbol(m).owner.expect("a member");
                    if !self.u.derives_from(winner_owner, owner) && self.same_member(site, winner, m) {
                        return self.unsupported_at(span, "conflicting inherited members");
                    }
                }
            }
        }
    }
}
