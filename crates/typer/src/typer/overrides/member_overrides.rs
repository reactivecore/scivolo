// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/RefChecks.scala
//! dotty's `checkOverride` for each member of a class and each base member it overrides.

use super::super::Typer;
use super::override_kinds::kind_refusal;
use super::{OverrideChecks, overridable};
use syntax::ast::flags as f;
use tasty::tags::{EXTENSION, FINAL, INLINE};
use tasty::{Kind, SymbolId, Type, nme};

impl OverrideChecks {
    /// Why a member of `own`, the class's declarations, may not override a member of a base
    /// class, if one may not: as a non-inline member overriding an inline method, a member of
    /// another `@targetName` or kind or with other empty parameter lists, a member without
    /// `override`, or an abstract member overriding a concrete one.
    pub fn member_refusal(&self, t: &mut Typer<'_>, own: Vec<SymbolId>) -> Option<&'static str> {
        let site = self.site;
        for d in own {
            let (name, kind) = (t.u.symbol(d).name.clone(), t.u.symbol(d).kind);
            let exempt = f::PRIVATE.or(f::SYNTHETIC);
            let getter = name.contains("$default$");
            let flags = t.syms.flags.get(&d).copied();
            if !kind.is_term()
                || kind == Kind::Param
                || name == nme::CONSTRUCTOR
                || getter
                || flags.is_none_or(|fl| fl.is_one_of(exempt))
            {
                continue;
            }
            let flags = flags.unwrap_or_default();
            for &b in &self.bases {
                for m in overridable(t.u, b, &name) {
                    let inline = |t: &Typer<'_>, s: SymbolId| t.u.symbol(s).flags.has(INLINE);
                    let inline_value = inline(t, d) && kind != Kind::Def;
                    let changed = inline(t, d) && t.inline_params(d) != t.inline_params(m);
                    let final_inline = inline(t, m) && t.is_concrete(m);
                    if (inline(t, m) && !inline(t, d) || inline_value || changed || final_inline)
                        && t.matches_loosely(site, d, m)
                    {
                        return Some(
                            "override of a concrete inline method, by an inline value or a non-inline \
                             member, or changing inline parameters",
                        );
                    }
                    t.complete(m);
                    if !t.matches_loosely(site, d, m) {
                        continue;
                    }
                    if t.u.symbol(d).run_name() != t.u.symbol(m).run_name() {
                        if !t.u.info_matches(site, d, m) {
                            continue;
                        }
                        return Some("override with another @targetName");
                    }
                    if let Some(what) = kind_refusal(t, d, m) {
                        return Some(what);
                    }
                    if t.overrides_other_nullary(self.cls, d, m) {
                        return Some("override changing empty parameter lists");
                    }
                    if !t.is_concrete(m) {
                        continue;
                    }
                    if flags.is(f::DEFERRED) {
                        if self.overrides_only_concretely(t, d, m) {
                            return Some("abstract member overriding a concrete one");
                        }
                        continue;
                    }
                    if !flags.is(f::OVERRIDE) && !flags.is(f::ABS_OVERRIDE) {
                        return Some("override without override modifier");
                    }
                }
            }
        }
        None
    }

    /// Whether abstract member `d` may not override concrete member `m` (dotty's `checkOverride`
    /// for a deferred member): `m` is final, of a final class or a value, one of them is an
    /// extension method, or `d`'s result type does not conform to `m`'s.
    fn overrides_only_concretely(&self, t: &mut Typer<'_>, d: SymbolId, m: SymbolId) -> bool {
        let (sd, sm) = (t.u.symbol(d), t.u.symbol(m));
        let final_owner = sm.owner.is_some_and(|o| t.u.symbol(o).flags.has(FINAL) || t.u.is_module_class(o));
        let refused = sm.flags.has(FINAL)
            || final_owner
            || sm.kind != Kind::Def
            || sd.flags.has(EXTENSION) != sm.flags.has(EXTENSION);
        let this = t.u.intern_type(Type::This(self.site));
        let (rd, rm) = (t.u.own_member_info(this, d), t.u.own_member_info(this, m));
        let binder = t.u.poly(rd).map(|p| p.binder);
        let (rd, mut rm) = t.u.aligned_results((d, rd), (m, rm));
        let params = t.u.type_params(d);
        if let Some(binder) = binder {
            rm = t.u.map_type(rm, &mut |u, x| {
                let Some((None, s)) = u.sym_ref(x) else { return None };
                params.iter().position(|&p| p == s).map(|i| u.intern_type(Type::Param(binder, i as u32)))
            });
        }
        refused || !t.conforms(rd, rm)
    }
}
