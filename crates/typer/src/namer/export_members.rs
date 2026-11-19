// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Namer.scala
//! Which members of an export's path get forwarders: those a wildcard selects, those that may
//! be forwarded at all (dotty's `addWildcardForwarders`, `canForward`), and default getters.

use super::exports::Export;
use crate::case_members::is_synthesized_override;
use crate::typer::Typer;
use names::{name_kinds, nme};
use syntax::ast::Span;
use tasty::tags as t;
use tasty::{Kind, SymbolId};

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
        let Some(class) = self.path_class(e.path) else {
            self.unsupported_at(span, "wildcard export from a path whose type is not a class");
            return;
        };
        let case_path = self.u.symbol(class).flags.has(t::CASE);
        for name in self.member_names_of(class) {
            for m in self.type_members(e.path, &name) {
                let flags = self.u.symbol(m).flags;
                if flags.has(t::PRIVATE)
                    || flags.has(t::SYNTHETIC)
                    || flags.has(t::ARTIFACT) && is_super_accessor_name(&name)
                    || name.starts_with("<local ")
                    || name.contains(name_kinds::DEFAULT_GETTER)
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

    fn is_case_synthesized(&self, m: SymbolId) -> bool {
        let s = self.u.symbol(m);
        s.owner.is_some_and(|o| is_synthesized_override(self.u, &s.name, o))
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
        let java_module = flags.has(t::OBJECT) && self.is_java_member(m);
        if name == nme::CONSTRUCTOR
            || kind == Kind::Class && flags.has(t::OBJECT)
            || flags.has(t::INVISIBLE)
            || java_module
        {
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
            for o in self.u.decls_named(base, &name) {
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
        let mut name = self.u.symbol(m).name.clone();
        let mut alias = alias;
        if self.is_case_apply(m) {
            name = nme::CONSTRUCTOR.to_owned();
            alias = nme::CONSTRUCTOR;
        }
        for n in defaults {
            let getter_name = name_kinds::default_getter(&name, n);
            let getter =
                self.type_members(e.path, &getter_name).into_iter().find(|&g| !self.u.is_type_symbol(g));
            let Some(getter) = getter else {
                self.unsupported_at(span, "export of a method without its default getter");
                return;
            };
            self.add_forwarder(e, &name_kinds::default_getter(alias, n), getter, span);
        }
    }

    /// Whether `m` is the synthesized `apply` of a case class's companion, which takes the
    /// constructor's default arguments.
    fn is_case_apply(&mut self, m: SymbolId) -> bool {
        let s = self.u.symbol(m);
        if s.name != nme::APPLY || !s.flags.has(t::SYNTHETIC) {
            return false;
        }
        let Some(owner) = s.owner else { return false };
        self.u.companion_class(owner).is_some_and(|c| self.u.symbol(c).flags.has(t::CASE))
    }
}

/// Whether `name`, expanded as `p$C$$x` or not, is a super accessor's: `super$m` or `m$super$Q`
/// (dotty's `isSuperAccessor`).
fn is_super_accessor_name(name: &str) -> bool {
    let unexpanded = name.rsplit_once(name_kinds::EXPAND_SEPARATOR).map_or(name, |(_, x)| x);
    name_kinds::decompose_super(unexpanded).is_some()
}

#[cfg(test)]
mod tests {
    use super::is_super_accessor_name;

    #[test]
    fn super_accessor_names_plain_mixed_and_expanded() {
        for name in ["super$m", "m$super$Q", "p$T$$super$m", "p$T$$m$super$Q"] {
            assert!(is_super_accessor_name(name), "{name}");
        }
        assert!(!is_super_accessor_name("m"));
    }
}
