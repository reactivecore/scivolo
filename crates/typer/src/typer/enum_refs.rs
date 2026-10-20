// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/ast/DesugarEnums.scala
//! Type references the desugaring made to enum classes by their full names.

use super::Typer;
use crate::ctx::Ctx;
use tasty::tags::{CONTRAVARIANT, COVARIANT};
use tasty::{Kind, SymbolId, Type, TypeId};

impl Typer<'_> {
    /// The type of full name `full`; for an enum parent `E[...]`, the enum class applied to the
    /// bounds of its type parameters (dotty's `interpolatedEnumParent`).
    pub fn full_type(&mut self, full: &str, ctx: Ctx) -> Option<TypeId> {
        let Some((tycon, _)) = full.split_once('[') else {
            let s = self.full_class(full, ctx)?;
            return Some(crate::defn::type_ref(self.u, s));
        };
        let class = self.full_class(tycon, ctx)?;
        self.complete(class);
        let tparams = self.u.type_params(class);
        let mut args = Vec::new();
        for &p in &tparams {
            let info = self.u.info(p);
            let Type::Bounds(lo, hi) = *self.u.types().get(info) else { return None };
            let flags = self.u.symbol(p).flags;
            let arg = match (flags.has(COVARIANT), flags.has(CONTRAVARIANT)) {
                (true, false) => lo,
                (false, true) => hi,
                _ => return None,
            };
            let mut ground = true;
            self.u.map_type(arg, &mut |u, t| {
                if let Type::Sym(_, s) = *u.types().get(t) {
                    ground &= !tparams.contains(&s);
                }
                None
            });
            args.push(ground.then_some(arg)?);
        }
        Some(crate::defn::applied(self.u, class, args))
    }

    /// The class of full name `full`; for an enum class the desugaring named without its packages
    /// or with its local owners (`Outer._$E`), the class of its simple name visible in `ctx` if
    /// the desugaring gives it that name.
    fn full_class(&mut self, full: &str, ctx: Ctx) -> Option<SymbolId> {
        if let Some(s) = self.resolve_full_name(full, true) {
            return Some(s);
        }
        let simple = full.rsplit_once('.').map_or(full, |(_, s)| s).trim_start_matches("_$");
        self.class_named(simple, ctx).filter(|&c| self.desugared_name(c) == full)
    }

    /// The full name the desugaring gives class `c`: its owners' names, a `_$` per term owner,
    /// `<empty>` for a local class outside classes.
    fn desugared_name(&self, c: SymbolId) -> String {
        let name = &self.u.symbol(c).name;
        let mut terms = 0;
        let mut owner = self.u.symbol(c).owner;
        let prefix = loop {
            let Some(o) = owner else { break None };
            match self.u.symbol(o).kind {
                Kind::Class => break Some(self.desugared_name(o)),
                Kind::Package => {
                    let package = self.u.full_name(o);
                    break (!matches!(package.as_str(), "" | "<empty>")).then_some(package);
                }
                _ => terms += 1,
            }
            owner = self.u.symbol(o).owner;
        };
        match (prefix, terms) {
            (None, 0) => name.clone(),
            (Some(p), 0) => format!("{p}.{name}"),
            (p, t) => format!("{}.{}{name}", p.as_deref().unwrap_or("<empty>"), "_$".repeat(t)),
        }
    }
}
