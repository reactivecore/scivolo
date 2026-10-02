//! Member types as seen from a prefix, normalized: class type parameters resolved through the
//! prefix's base type, applied type lambdas reduced, capture annotations dropped.

use super::Typer;
use tasty::{Kind, Type, TypeId};

impl Typer<'_> {
    /// The type of the method `fun_tpe` refers to, as seen from its prefix.
    pub fn method_info(&mut self, fun_tpe: TypeId) -> Option<TypeId> {
        let Type::Sym(prefix, s) = self.u.types().get(fun_tpe).clone() else { return None };
        self.complete(s);
        let seen_from_prefix = prefix.filter(|&p| !matches!(self.u.types().get(p), Type::Package(_)));
        let seen_from_prefix = seen_from_prefix.map(|p| self.aliased_object(p));
        let info = match seen_from_prefix {
            Some(p) => self.u.member_info(p, s),
            None => self.u.info(s),
        };
        let info = self.resolve_class_params(info);
        Some(self.with_repeated_class(info))
    }

    /// The object that `p`, a reference to a value whose type is a reference to an object (such
    /// as `scala.package.List`), stands for; `p` otherwise.
    fn aliased_object(&mut self, mut p: TypeId) -> TypeId {
        for _ in 0..8 {
            let Type::Sym(_, s) = *self.u.types().get(p) else { break };
            let sym = self.u.symbol(s);
            if sym.kind != Kind::Val
                || sym.flags.has(tasty::tags::OBJECT)
                || sym.flags.has(tasty::tags::MUTABLE)
            {
                break;
            }
            self.complete(s);
            let info = self.u.info(s);
            match *self.u.types().get(info) {
                Type::Sym(_, o) if self.u.symbol(o).flags.has(tasty::tags::OBJECT) => p = info,
                Type::TermRef(..) => p = info,
                _ => break,
            }
        }
        p
    }

    /// `t` with each type parameter of a class `C` selected on a prefix other than `this`
    /// replaced by the argument the prefix's base type `C[..]` gives it, then normalized.
    pub fn resolve_class_params(&mut self, t: TypeId) -> TypeId {
        let t = self.u.map_type(t, &mut |u, part| {
            let Type::Sym(Some(pre), p) = *u.types().get(part) else { return None };
            if u.symbol(p).kind != Kind::TypeParam || matches!(u.types().get(pre), Type::This(_)) {
                return None;
            }
            let class = u.symbol(p).owner.filter(|&c| u.symbol(c).kind == Kind::Class)?;
            let index = u.type_params(class).iter().position(|&q| q == p)?;
            let widened = u.widen(pre);
            match u.base_type(widened, class).map(|b| u.types().get(b).clone()) {
                Some(Type::Applied(_, args)) => args.get(index).copied(),
                _ => None,
            }
        });
        self.normalized(t)
    }

    /// `t` with applied type lambdas reduced and capture-checking annotations (`retains`)
    /// dropped, which dotty leaves out without capture checking.
    pub fn normalized(&mut self, t: TypeId) -> TypeId {
        let retains = |u: &mut tasty::Universe, a| {
            u.class_symbol(a).is_some_and(|c| u.full_name(c).starts_with("scala.annotation.retains"))
        };
        self.u.map_type(t, &mut |u, part| {
            if let Type::Annotated(inner, a) = *u.types().get(part)
                && retains(u, a)
            {
                return Some(inner);
            }
            let Type::Applied(tycon, args) = u.types().get(part).clone() else { return None };
            let Type::Lambda { binder, params, result } = u.types().get(tycon).clone() else { return None };
            let names: Vec<String> = params.into_iter().map(|(n, _)| n).collect();
            Some(u.subst(result, binder, &names, &args))
        })
    }
}
