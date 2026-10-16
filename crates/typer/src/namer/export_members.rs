//! Which members of an export's path get forwarders: those a wildcard selects, those that may
//! be forwarded at all (dotty's `addWildcardForwarders`, `canForward`), and default getters.

use super::exports::Export;
use crate::typer::Typer;
use syntax::ast::Span;
use tasty::tags as t;
use tasty::{Kind, SymbolId, Type, TypeId};

/// The members of a case class path that dotty's `PostTyper` synthesizes.
const CASE_SYNTHESIZED: [&str; 8] = [
    "hashCode",
    "equals",
    "toString",
    "canEqual",
    "productArity",
    "productPrefix",
    "productElement",
    "productElementName",
];

impl Typer<'_> {
    /// Adds the forwarders of a wildcard selector at `span` of `e`, of givens if `given`, of the
    /// other members if `plain`, but of the names `seen` named before.
    pub fn add_wildcard_forwarders(
        &mut self,
        e: &mut Export,
        seen: &[String],
        (given, plain): (bool, bool),
        span: Span,
    ) {
        let mut non_contextual: Vec<String> = seen.to_vec();
        let class = self.path_class(e.path);
        let case_path = class.is_some_and(|c| self.u.symbol(c).flags.has(t::CASE));
        for name in class.map(|c| self.member_names_of(c)).unwrap_or_default() {
            for m in self.type_members(e.path, &name) {
                let flags = self.u.symbol(m).flags;
                if flags.has(t::PRIVATE)
                    || flags.has(t::SYNTHETIC)
                    || name.starts_with("super$")
                    || name.contains("$default$")
                    || case_path && self.is_case_synthesized(m)
                {
                    continue;
                }
                if flags.has(t::GIVEN) {
                    if given && !seen.contains(&name) {
                        self.add_forwarder(e, &name, m, span);
                    }
                } else if plain && !non_contextual.contains(&name) {
                    non_contextual.push(name.clone());
                    for alt in self.terms_then_types(e.path, &name) {
                        let flags = self.u.symbol(alt).flags;
                        if !flags.has(t::PRIVATE) && !flags.has(t::GIVEN) {
                            self.add_forwarder(e, &name, alt, span);
                        }
                    }
                }
            }
        }
    }

    /// The members named `name` of path type `t`, the terms first.
    pub fn terms_then_types(&mut self, t: TypeId, name: &str) -> Vec<SymbolId> {
        let mut members = self.type_members(t, name);
        members.sort_by_key(|&m| self.u.is_type_symbol(m));
        members
    }

    /// The class whose members a path of type `t` has.
    fn path_class(&mut self, t: TypeId) -> Option<SymbolId> {
        if let Type::Sym(_, s) = *self.u.types().get(t)
            && self.u.symbol(s).flags.has(t::OBJECT)
        {
            return self.u.module_class(s);
        }
        let widened = self.u.widen(t);
        self.u.class_symbol(widened)
    }

    /// The names of the members of class `c`, its own first, then those of its base classes.
    fn member_names_of(&mut self, c: SymbolId) -> Vec<String> {
        self.complete(c);
        let mut names: Vec<String> = Vec::new();
        for base in self.u.linearization(c) {
            self.complete(base);
            for d in self.u.decls(base).unwrap_or_default() {
                let name = &self.u.symbol(d).name;
                if !names.contains(name) {
                    names.push(name.clone());
                }
            }
        }
        names
    }

    fn is_case_synthesized(&self, m: SymbolId) -> bool {
        let owner = self.u.symbol(m).owner.map(|o| self.u.full_name(o)).unwrap_or_default();
        CASE_SYNTHESIZED.contains(&self.u.symbol(m).name.as_str())
            && ["scala.Any", "java.lang.Object", "scala.Product", "scala.Equals"].contains(&owner.as_str())
    }

    /// Whether member `m` gets a forwarder named `alias` in `e` (dotty's `canForward`); `None`
    /// if it is skipped silently.
    pub fn can_forward(&mut self, e: &Export, m: SymbolId, alias: &str) -> Option<bool> {
        let (name, kind, flags, owner) = {
            let s = self.u.symbol(m);
            (s.name.clone(), s.kind, s.flags, s.owner)
        };
        if !self.is_accessible(m, e.ctx) {
            return Some(false);
        }
        if name == "<init>" || kind == Kind::Class && flags.has(t::OBJECT) || flags.has(t::INVISIBLE) {
            return None;
        }
        let owner = owner?;
        let deferred = !self.is_concrete(m) && (self.syms.is_script(m) || self.u.symbol(m).tree.is_some());
        if self.u.derives_from(e.cls, owner) && (owner == e.cls || !deferred) {
            return Some(false);
        }
        if e.path_method.is_some() && (kind.is_type() || flags.has(t::EXTENSION)) {
            return Some(false);
        }
        if e.has_target(alias) {
            return Some(false);
        }
        if flags.has(t::OVERRIDE) && self.overrides_inherited(e.cls, m) {
            return Some(false);
        }
        Some(true)
    }

    /// Whether `m` overrides a concrete member of a base class of `cls`.
    fn overrides_inherited(&mut self, cls: SymbolId, m: SymbolId) -> bool {
        let Some(owner) = self.u.symbol(m).owner else { return false };
        let name = self.u.symbol(m).name.clone();
        let site = crate::defn::type_ref(self.u, owner);
        for base in self.u.linearization(owner).into_iter().skip(1) {
            if !self.u.derives_from(cls, base) {
                continue;
            }
            for o in self.u.decl(base, &name).unwrap_or_default() {
                if !self.u.is_type_symbol(o) && self.is_concrete(o) && self.same_member(site, m, o) {
                    return true;
                }
            }
        }
        false
    }

    /// Adds the forwarders of the default getters of method `m`, forwarded as `alias` (dotty's
    /// `foreachDefaultGetterOf`).
    pub fn add_default_forwarders(&mut self, e: &mut Export, m: SymbolId, alias: &str, span: Span) {
        let params: Vec<SymbolId> = self.u.decls(m).unwrap_or_default();
        let params = params.into_iter().filter(|&p| self.u.symbol(p).kind == Kind::Param);
        let defaults: Vec<usize> = params
            .enumerate()
            .filter(|&(_, p)| self.u.symbol(p).flags.has(t::HASDEFAULT))
            .map(|(n, _)| n + 1)
            .collect();
        if defaults.is_empty() {
            return;
        }
        let name = self.u.symbol(m).name.clone();
        if name == "apply" && self.u.symbol(m).flags.has(t::SYNTHETIC) {
            self.unsupported_at(span, "export of a synthesized apply with default arguments");
            return;
        }
        for n in defaults {
            let getter_name = format!("{name}$default${n}");
            let getter =
                self.type_members(e.path, &getter_name).into_iter().find(|&g| !self.u.is_type_symbol(g));
            let Some(getter) = getter else {
                self.unsupported_at(span, "export of a method without its default getter");
                return;
            };
            self.add_forwarder(e, &format!("{alias}$default${n}"), getter, span);
        }
    }
}
