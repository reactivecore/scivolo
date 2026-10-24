// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/RefChecks.scala
//! What dotty's `RefChecks.checkAllOverrides` rejects and the typer does not port yet, made to fall
//! back: a member overriding a concrete one without `override` or by an abstract one.

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
    pub fn overridable(&self, c: SymbolId, name: &str) -> Vec<SymbolId> {
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
                        if self.overrides_only_concretely(d, m) {
                            return self.unsupported_at(span, "abstract member overriding a concrete one");
                        }
                        continue;
                    }
                    if !flags.is(f::OVERRIDE) && !flags.is(f::ABS_OVERRIDE) {
                        return self.unsupported_at(span, "override without override modifier");
                    }
                }
            }
        }
        self.check_inherited_conflicts(cls, site, &bases, span);
    }

    /// Whether abstract member `d` may not override concrete member `m` (dotty's `checkOverride`
    /// for a deferred member): `m` is not `Object`'s, is final, of a final class or a value, one
    /// of them is an extension method, or `d`'s result type does not conform to `m`'s.
    fn overrides_only_concretely(&mut self, d: SymbolId, m: SymbolId) -> bool {
        use tasty::tags::{EXTENSION, FINAL};
        let (sd, sm) = (self.u.symbol(d), self.u.symbol(m));
        let final_owner =
            sm.owner.is_some_and(|o| self.u.symbol(o).flags.has(FINAL) || self.u.is_module_class(o));
        let object = sm.owner.is_some_and(|o| self.u.full_name(o) == "java.lang.Object");
        let refused = !object
            || sm.flags.has(FINAL)
            || final_owner
            || sm.kind != Kind::Def
            || sd.flags.has(EXTENSION) != sm.flags.has(EXTENSION);
        let (rd, rm) = (self.u.info(d), self.u.info(m));
        let (rd, rm) = (self.u.result_type(rd), self.u.result_type(rm));
        refused || !self.conforms(rd, rm)
    }
}
