//! `new Array[T](n)` of a generic `T` as `scala.runtime.Arrays.newGenericArray[T](n)`, which
//! takes a class tag (dotty's `convertNewGenericArray`).

use super::Typer;
use crate::tpd::{TNodeId, Tree as T};
use tasty::tags::OPAQUE;
use tasty::{Kind, Type, TypeId};

impl Typer<'_> {
    pub fn converted_new_generic_array(&mut self, tree: TNodeId) -> Result<TNodeId, &'static str> {
        let T::Apply { fun, args } = *self.tpd.tree(tree) else { return Ok(tree) };
        let T::TypeApply { fun: ctor, args: targs } = *self.tpd.tree(fun) else { return Ok(tree) };
        let &[targ] = self.tpd.items(targs) else { return Ok(tree) };
        let array = self.defn.array;
        let Some(s) = self.tpd.sym(ctor).filter(|&s| self.u.symbol(s).name == "<init>") else {
            return Ok(tree);
        };
        if self.u.symbol(s).owner != Some(array) {
            return Ok(tree);
        }
        let tpe = self.tpd.tpe(tree);
        self.fully_defined(tpe)?;
        let elem = self.zonk(self.tpd.tpe(targ));
        if !self.is_generic_array_arg(elem) {
            return Ok(tree);
        }
        let module = self.resolve_full_name("scala.runtime.Arrays", false).ok_or("scala.runtime.Arrays")?;
        let method = self.resolve_full_name("scala.runtime.Arrays$.newGenericArray", false);
        let method = method.ok_or("scala.runtime.Arrays.newGenericArray")?;
        let span = self.tpd.span(tree);
        let qual = self.build().static_ident(module, span.start_pos());
        let select = self.build().select(qual, method, span);
        let poly = self.instantiated(select, &[elem])?;
        let Type::Method { result, .. } = *self.u.types().get(poly) else { return Err("newGenericArray") };
        let type_apply = self.build().type_apply(select, &[targ], poly, span);
        let args = self.tpd.items(args).to_vec();
        Ok(self.build().apply(type_apply, &args, result, span))
    }

    /// Whether `t` is `Array[...[U]]` of a generic `U` (dotty's `isGenericArrayArg`).
    fn is_generic_array_arg(&mut self, t: TypeId) -> bool {
        let t = self.u.dealias(t);
        match self.u.types().get(t).clone() {
            Type::Applied(tycon, args) if self.u.class_symbol(tycon) == Some(self.defn.array) => {
                args.first().is_some_and(|&a| self.is_generic_array_arg(a))
            }
            _ => self.is_generic(t, 16),
        }
    }

    /// Whether `t` is an abstract type, a type parameter, or another generic type (dotty's
    /// `TypeErasure.isGeneric`).
    fn is_generic(&mut self, t: TypeId, depth: u32) -> bool {
        let Some(depth) = depth.checked_sub(1) else { return false };
        let t = self.u.dealias(t);
        match self.u.types().get(t).clone() {
            Type::Sym(_, s) if self.u.symbol(s).flags.has(OPAQUE) => {
                let info = self.u.info(s);
                let alias = match *self.u.types().get(info) {
                    Type::Alias(a) => Some(a),
                    _ => None,
                };
                alias.is_some_and(|a| self.is_generic(a, depth))
            }
            Type::Sym(_, s) if self.u.is_type_symbol(s) => self.u.symbol(s).kind != Kind::Class,
            Type::Param(..) | Type::Local(..) => true,
            Type::And(a, b) | Type::Or(a, b) => self.is_generic(a, depth) || self.is_generic(b, depth),
            Type::Annotated(a, _) => self.is_generic(a, depth),
            Type::Applied(tycon, _) => match *self.u.types().get(tycon) {
                Type::Sym(_, s) if matches!(self.u.symbol(s).kind, Kind::Type | Kind::TypeParam) => {
                    self.complete(s);
                    let info = self.u.info(s);
                    let hi = match *self.u.types().get(info) {
                        Type::Bounds(_, hi) | Type::Alias(hi) => hi,
                        _ => return true,
                    };
                    match self.u.types().get(hi).clone() {
                        Type::Lambda { result, .. } => self.is_generic(result, depth),
                        _ => self.is_generic(hi, depth),
                    }
                }
                _ => self.is_generic(tycon, depth),
            },
            _ => false,
        }
    }
}
