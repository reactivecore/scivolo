//! Selections in a retyped inline body: the members dotty's retyping would select instead, and
//! the prefixes a selection keeps.

use super::{Inlining, Unsupported};
use crate::tpd::{TNodeId, Tree as T};
use crate::typer::dependent_params::applied_result;
use crate::typer::type_apply::instantiate_poly;
use tasty::tags::PRIVATE;
use tasty::{SymbolId, Type, TypeId};

impl Inlining<'_> {
    /// Makes selection `t` of `name` from retyped `qualifier` select the member dotty's retyping
    /// selects instead (`selectionType`), if there is one.
    pub fn reselect(&mut self, t: TNodeId, qualifier: TNodeId, name: String) -> Result<(), Unsupported> {
        let refused = (self.tpd.span(t), "inline expansion selecting an overriding member");
        if let Some(m) = self.reselected(t, qualifier, name).map_err(|()| refused)? {
            let prefix = self.tpd.tpe(qualifier);
            let tpe = crate::defn::ref_type(self.u, Some(prefix), m);
            let node = self.tpd.get_mut(t);
            (node.sym, node.tpe) = (Some(m), Some(tpe));
        }
        Ok(())
    }

    /// The member named `name` that selection `t` from `qualifier` finds in the class of the
    /// qualifier's type before the owner of the selected symbol with the selected symbol's
    /// signature, abstract ones only for an abstract selected symbol: none if there is none, `Err`
    /// if there are several or it is private.
    fn reselected(&mut self, t: TNodeId, qualifier: TNodeId, name: String) -> Result<Option<SymbolId>, ()> {
        let (Some(sym), Some(q)) = (self.tpd.sym(t), self.tpd.get(qualifier).tpe) else { return Ok(None) };
        let owner = self.u.symbol(sym).owner;
        let concrete = !self.is_abstract(sym);
        let widened = self.u.widen(q);
        let Some(c) = self.u.class_symbol(widened) else { return Ok(None) };
        for base in self.u.linearization(c) {
            if Some(base) == owner {
                return Ok(None);
            }
            let decls = self.u.decls(base).unwrap_or_default();
            let site = crate::defn::type_ref(self.u, c);
            let same: Vec<SymbolId> = (decls.into_iter())
                .filter(|&d| {
                    self.u.symbol(d).name == name
                        && self.u.symbol(d).kind.is_term()
                        && !(concrete && self.is_abstract(d))
                        && self.u.matches(site, d, sym)
                })
                .collect();
            let public = same.iter().all(|&d| !self.u.symbol(d).flags.has(PRIVATE));
            match same[..] {
                [] => continue,
                [d] if public => return Ok(Some(d)),
                _ => return Err(()),
            }
        }
        Ok(None)
    }

    /// Gives application `t` of `fun`, if its method part selects another member than `before`,
    /// the type of applying `fun`'s retyped method type (dotty's `typedApply` and `typedTypeApply`).
    pub fn reapply(&mut self, t: TNodeId, before: Option<SymbolId>) {
        let (fun, args) = match self.tpd.tree(t) {
            T::Apply { fun, args } | T::TypeApply { fun, args } => (*fun, *args),
            _ => return,
        };
        let applied = matches!(self.tpd.tree(t), T::Apply { .. });
        let meth = self.tpd.sym(self.tpd.meth_part(fun));
        if meth == before {
            return;
        }
        let info = self.tpd.tpe(fun);
        let info = self.u.widen(info);
        let args: Vec<TypeId> = self.tpd.items(args).iter().map(|&a| self.tpd.tpe(a)).collect();
        let tpe = match self.u.types().get(info).clone() {
            Type::Method { binder, params, result, .. } if applied && params.len() == args.len() => {
                let names: Vec<String> = params.into_iter().map(|(n, _)| n).collect();
                applied_result(self.u, result, (binder, &names), meth, &args)
            }
            Type::Poly { binder, params, result } if !applied && params.len() == args.len() => {
                let names: Vec<String> = params.into_iter().map(|(n, _)| n).collect();
                instantiate_poly(self.u, result, (binder, &names), &args, meth)
            }
            _ => return,
        };
        self.tpd.get_mut(t).tpe = Some(tpe);
    }

    /// Whether member `s` has no body: deferred in the source or without a right-hand side in TASTy.
    fn is_abstract(&self, s: SymbolId) -> bool {
        self.u.is_deferred(s) || (self.u.symbol(s).tree.is_some() && self.u.body(s).is_none())
    }

    /// The prefix of a selection from a qualifier of type `t` (dotty's `cpy.Select`): `t` if it is
    /// a reference to a stable local term or a `this`, the result type if it is one to a by-name
    /// local term.
    pub fn local_prefix(&mut self, t: TypeId) -> Option<TypeId> {
        match *self.u.types().get(t) {
            Type::This(_) => Some(t),
            Type::Sym(None, s) if self.u.symbol(s).kind.is_term() => {
                if self.pure().is_stable_member(s) {
                    return Some(t);
                }
                let info = self.u.info(s);
                match *self.u.types().get(info) {
                    Type::ByName(r) => Some(r),
                    _ => None,
                }
            }
            _ => None,
        }
    }
}
