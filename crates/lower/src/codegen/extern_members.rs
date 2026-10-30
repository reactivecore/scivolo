// Derived from Scala Native 0.5.12 (Apache-2.0, see native/LICENSE-scala-native.md); translated to Rust and changed:
//   nscplugin/src/main/scala-3/scala/scalanative/nscplugin/NirGenStat.scala
//! Methods of extern objects and traits (nscplugin's `genExternMethod`): declarations, forwarders
//! to them, and runtime overloads, which are ordinary methods.

use super::Gen;
use crate::phases::casts::class_named;
use crate::symbols::{has, is_extern};
use crate::{Result, unsupported};
use nir::{Attr, Defn, DefnKind, Type};
use tasty::tags::{FIELDaccessor, SYNTHETIC};
use typer::tpd::{TNodeId, Tree};

const NON_EXTERN: &str = "scala.scalanative.annotation.nonExtern";

impl Gen<'_, '_> {
    /// `def m(...): T = extern`, or a forwarder to such a method, as a declaration of `m`; a
    /// runtime overload as an ordinary method typed by its C parameters, without a receiver, and
    /// its result. None for a synthetic method.
    pub fn gen_extern_method(&mut self, dd: TNodeId) -> Result<Option<Defn>> {
        let sym = self.l.tpd.sym(dd).expect("a method definition's symbol");
        let Tree::DefDef { rhs, .. } = *self.l.tpd.tree(dd) else { unreachable!("a method") };
        let span = self.l.tpd.span(dd);
        let Some(rhs) = rhs else { return unsupported(span, "abstract methods in extern objects") };
        let called = match *self.l.tpd.tree(rhs) {
            Tree::Apply { fun, .. } => self.l.tpd.sym(fun).filter(|&t| is_extern(self.l.u, t)),
            _ => None,
        };
        let target = match called {
            _ if self.is_extern_body(rhs) => sym,
            _ if has(self.l.u, sym, SYNTHETIC) && !self.l.bridges.contains(&sym) => return Ok(None),
            _ if has(self.l.u, sym, FIELDaccessor) && !self.l.bridges.contains(&sym) => return Ok(None),
            _ if self.l.forwarded.contains_key(&sym) => return Ok(None),
            Some(t) if self.extern_id(t, span)? == self.extern_id(sym, span)? => t,
            _ => return unsupported(span, "methods in extern objects without an extern body"),
        };
        let ty = self.extern_sig(target, span)?;
        if target != sym {
            let own = self.extern_sig(sym, span)?;
            if !self.matches_extern_sig(own, ty) {
                let class = class_named(self.l, NON_EXTERN).expect("Scala Native's nonExtern");
                self.l.u.annotate(sym, class);
                let mut defn = self.gen_method(dd)?;
                let Type::Function(args, _) = self.pool.ty(own).clone() else { unreachable!("a signature") };
                let ret = self.result_type(sym);
                if let DefnKind::Define { ty, .. } = &mut defn.kind {
                    *ty = self.ty(Type::Function(args, ret));
                }
                return Ok(Some(defn));
            }
        }
        let inline = has(self.l.u, sym, FIELDaccessor) || self.l.bridges.contains(&sym);
        let owner = self.l.u.symbol(sym).owner.expect("a method's owner");
        let extern_ = Attr::Extern { blocking: self.is_blocking(sym) || self.is_blocking(owner) };
        let attrs = if inline { vec![Attr::AlwaysInline, extern_] } else { vec![extern_] };
        let attrs = self.method_attrs(sym, attrs);
        let pos = self.pos_or(self.l.tpd.span(rhs), self.no_pos);
        let name = self.extern_name(owner, target, span)?;
        Ok(Some(Defn { name, attrs, pos, kind: DefnKind::Declare { ty } }))
    }

    /// Whether a forwarder of C signature `own` calls an extern method of C signature `target`
    /// with the same arguments, a final vararg taking the forwarder's last parameter.
    fn matches_extern_sig(&self, own: nir::TypeId, target: nir::TypeId) -> bool {
        let (Type::Function(own_args, own_ret), Type::Function(args, ret)) =
            (self.pool.ty(own), self.pool.ty(target))
        else {
            return false;
        };
        let varargs = args.last().is_some_and(|&a| *self.pool.ty(a) == Type::Vararg);
        let args_match = match varargs {
            true => args.len() == own_args.len() && args[..args.len() - 1] == own_args[..own_args.len() - 1],
            false => args == own_args,
        };
        args_match && ret == own_ret
    }
}
