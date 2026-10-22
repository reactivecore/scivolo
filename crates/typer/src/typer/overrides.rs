// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/RefChecks.scala
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
            None => self.u.body(s).is_some() || self.u.symbol(s).owner == Some(self.u.builtins().any),
        }
    }

    /// The name `s` has at run time: its `@targetName`, else its name.
    pub fn run_name(&self, s: SymbolId) -> &str {
        let s = self.u.symbol(s);
        s.target.as_deref().unwrap_or(&s.name)
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
        match (ka, kb) {
            (Kind::Def, Kind::Def) => self.u.matches(site, a, b) || self.is_nullary(a) && self.is_nullary(b),
            (Kind::Def, _) => self.is_nullary(a),
            (_, Kind::Def) => self.is_nullary(b),
            _ => true,
        }
    }

    /// Whether method `s` takes no parameters or one empty non-implicit clause and nothing else.
    pub fn is_nullary(&mut self, s: SymbolId) -> bool {
        use crate::symbols::Completion;
        if let Some(&(Completion::Pending { tree, .. } | Completion::Completing(tree))) =
            self.syms.completions.get(&s)
        {
            return self.is_nullary_def(tree);
        }
        let info = self.u.info(s);
        match self.u.types().get(info) {
            Type::Method { params, result, implicit: false, .. } if params.is_empty() => {
                !matches!(self.u.types().get(*result), Type::Method { .. } | Type::Poly { .. })
            }
            Type::Method { .. } | Type::Poly { .. } => false,
            _ => true,
        }
    }

    /// Whether method definition `tree` has no parameters or one empty clause and nothing else.
    pub fn is_nullary_def(&self, tree: syntax::ast::NodeId) -> bool {
        let clauses = self.def_clauses(tree);
        clauses.len() <= 1 && clauses.concat().is_empty()
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
                if self.run_name(a) == self.run_name(b) && self.u.matches(site, a, b) {
                    return self.unsupported_at(span, "double definition");
                }
                if self.has_default_args(a) && self.has_default_args(b) {
                    return self.unsupported_at(span, "overloads with default arguments");
                }
            }
        }
        self.check_no_clashes(cls, span);
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
                    let inline = |t: &Self, s: SymbolId| t.u.symbol(s).flags.has(tasty::tags::INLINE);
                    let inline_value = inline(self, d) && kind != Kind::Def;
                    let changed = inline(self, d) && self.inline_params(d) != self.inline_params(m);
                    let final_inline = inline(self, m) && self.is_concrete(m);
                    if (inline(self, m) && !inline(self, d) || inline_value || changed || final_inline)
                        && self.same_member(site, d, m)
                    {
                        return self.unsupported_at(
                            span,
                            "override of a concrete inline method, by an inline value or a non-inline \
                             member, or changing inline parameters",
                        );
                    }
                    self.complete(m);
                    if self.run_name(d) != self.run_name(m) && self.same_member(site, d, m) {
                        return self.unsupported_at(span, "override with another @targetName");
                    }
                    if !self.is_concrete(m) || !self.same_member(site, d, m) {
                        continue;
                    }
                    if flags.is(f::DEFERRED) {
                        return self.unsupported_at(span, "abstract member overriding a concrete one");
                    }
                    if !flags.is(f::OVERRIDE) && !flags.is(f::ABS_OVERRIDE) {
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
                    if self.u.derives_from(winner_owner, owner) || !self.same_member(site, winner, m) {
                        continue;
                    }
                    if !self.overrides_through(site, winner, (winner_owner, owner))
                        && !self.overrides_through(site, m, (owner, winner_owner))
                    {
                        return self.unsupported_at(span, "conflicting inherited members");
                    }
                }
            }
        }
    }

    /// Whether member `m` of class `a`, declared `override`, overrides the member of class `b`.
    fn overrides_through(&mut self, site: TypeId, m: SymbolId, (a, b): (SymbolId, SymbolId)) -> bool {
        let overrides = match self.syms.flags.get(&m) {
            Some(fl) => fl.is(f::OVERRIDE) || fl.is(f::ABS_OVERRIDE),
            None => self.u.symbol(m).flags.has(tasty::tags::OVERRIDE),
        };
        overrides && self.overrides_in_common(site, (a, b), m)
    }

    /// Whether classes `a` and `b` both derive from a third class with a member `m` matches, or
    /// `b` is a base class of `a`'s self type (dotty's `extendedOverriddenSymbols`).
    fn overrides_in_common(&mut self, site: TypeId, (a, b): (SymbolId, SymbolId), m: SymbolId) -> bool {
        let name = self.u.symbol(m).name.clone();
        if self.self_type_bases(a).contains(&b) {
            return true;
        }
        let bases: Vec<SymbolId> = self.u.linearization(a).into_iter().skip(1).collect();
        let common: Vec<SymbolId> =
            bases.into_iter().filter(|&c| c != b && self.u.derives_from(b, c)).collect();
        common.into_iter().any(|c| {
            let members = self.overridable(c, &name);
            members.into_iter().any(|o| self.same_member(site, m, o))
        })
    }

    /// The base classes of the classes in the given self type of class `c`.
    fn self_type_bases(&mut self, c: SymbolId) -> Vec<SymbolId> {
        let Some(self_type) = self.u.given_self_type(c) else { return Vec::new() };
        let mut todo = vec![self_type];
        let mut out = Vec::new();
        while let Some(t) = todo.pop() {
            if let Type::And(x, y) = *self.u.types().get(t) {
                todo.extend([x, y]);
            } else if let Some(class) = self.u.class_symbol(t) {
                out.extend(self.u.linearization(class));
            }
        }
        out
    }
}
