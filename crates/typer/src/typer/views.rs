// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Implicits.scala
//! Whether an implicit conversion might turn one type into another (dotty's `viewExists`),
//! decided without implicit search: by the result and parameter classes of the implicit methods
//! the imports and the companions of both types offer.

use super::Typer;
use super::imports::ImportInfo;
use crate::ctx::{Ctx, Level};
use syntax::ast::flags as f;
use tasty::tags::{GIVEN, IMPLICIT};
use tasty::{Kind, SymbolId, Type, TypeId};

impl Typer<'_> {
    /// Whether a view from `arg` to `param` may be visible in `ctx`: `false` only when none is.
    pub fn view_may_exist(&mut self, arg: TypeId, param: TypeId, ctx: Ctx) -> bool {
        if self.syms.flags.values().any(|fl| fl.is(f::IMPLICIT) || fl.is(f::GIVEN)) {
            return true;
        }
        let widened = self.u.widen(arg);
        let (Some(from), Some(to)) = (self.u.class_symbol(widened), self.u.class_symbol(param)) else {
            return true;
        };
        let mut candidates = Vec::new();
        for c in [from, to] {
            for base in self.u.linearization(c) {
                let module = self.u.companion_module(base);
                if let Some(companion) = module.and_then(|m| self.u.module_class(m)) {
                    candidates.extend(self.all_members(companion));
                }
            }
        }
        let mut level = Some(ctx.level);
        while let Some(l) = level {
            if let Level::Import(i) = *self.levels.get(l) {
                match self.imported_candidates(i) {
                    Some(syms) => candidates.extend(syms),
                    None => return true,
                }
            }
            level = self.levels.outer(l);
        }
        candidates.into_iter().any(|m| self.may_convert(m, from, to))
    }

    /// Whether `m` is a given, or an implicit method from a subclass of `from` to one of `to`.
    fn may_convert(&mut self, m: SymbolId, from: SymbolId, to: SymbolId) -> bool {
        let flags = self.u.symbol(m).flags;
        if flags.has(GIVEN) {
            return true;
        }
        if !flags.has(IMPLICIT) || self.u.symbol(m).kind != Kind::Def {
            return false;
        }
        let mut info = self.u.info(m);
        if let Type::Poly { result, .. } = self.u.types().get(info) {
            info = *result;
        }
        let Type::Method { params, mut result, implicit: false, .. } = self.u.types().get(info).clone()
        else {
            return false;
        };
        let [(_, view_param)] = params[..] else { return false };
        while let Type::Method { result: r, implicit: true, .. } = self.u.types().get(result) {
            result = *r;
        }
        let fits = |t: &mut Self, ty: TypeId, c: SymbolId, sub: bool| match t.u.class_symbol(ty) {
            Some(k) if sub => t.u.derives_from(c, k),
            Some(k) => t.u.derives_from(k, c),
            None => true,
        };
        fits(self, view_param, from, true) && fits(self, result, to, false)
    }

    /// The members import `i` makes visible, `None` if unknown.
    fn imported_candidates(&mut self, i: usize) -> Option<Vec<SymbolId>> {
        let ImportInfo { qual, selectors, .. } = self.import_info(i)?;
        let (qual, names) = (*qual, selectors.iter().map(|s| s.name.clone()).collect::<Vec<_>>());
        let site = self.import_site(qual)?;
        let classes = match self.u.symbol(site).kind {
            Kind::Package => self.u.package_objects(site).ok()?,
            _ => vec![self.u.module_class(site)?],
        };
        let mut out = Vec::new();
        for c in classes {
            let members = self.all_members(c);
            let wildcard = names.iter().any(|n| n == "_" || n.is_empty());
            out.extend(members.into_iter().filter(|&m| wildcard || names.contains(&self.u.symbol(m).name)));
        }
        Some(out)
    }

    /// The declarations of class `c` and its base classes.
    fn all_members(&mut self, c: SymbolId) -> Vec<SymbolId> {
        let mut out = Vec::new();
        for base in self.u.linearization(c) {
            out.extend(self.u.decls(base).unwrap_or_default());
        }
        out
    }
}
