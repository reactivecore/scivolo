//! The kinds of type arguments: type constructors where higher-kinded parameters are, class
//! constructors eta-expanded (dotty's `adaptType`).

use super::Typer;
use crate::symbols::Completion;
use crate::tpd::TNodeId;
use tasty::{Kind, Type, TypeId};

impl Typer<'_> {
    /// What about type arguments `args` of type constructor `tycon` taking `arity` is outside
    /// the subset: their number, a constructor where a parameter of another kind is, or a class
    /// constructor where the parameters are unknown.
    pub fn type_args_problem(
        &mut self,
        tycon: TypeId,
        arity: usize,
        args: &[TypeId],
    ) -> Option<&'static str> {
        if arity != args.len() {
            return Some("wrong number of type arguments");
        }
        for (i, &a) in args.iter().enumerate() {
            let n = self.type_arity(a);
            if matches!(self.u.types().get(a), Type::Bounds(..))
                && self.param_arity(tycon, i).is_some_and(|m| m > 0)
            {
                return Some("wildcard argument of a higher-kinded type parameter");
            }
            if n == 0 {
                continue;
            }
            match self.param_arity(tycon, i) {
                Some(m) if m == n => {}
                Some(_) => return Some("type argument of another kind"),
                None if matches!(*self.u.types().get(a), Type::Sym(_, s) if self.u.symbol(s).kind == Kind::Class) =>
                {
                    return Some("class type constructor argument");
                }
                None => {}
            }
        }
        None
    }

    /// Type argument `arg` as dotty's `adaptType` leaves it: a class constructor eta-expanded.
    pub fn adapted_type_arg(&mut self, arg: TNodeId) -> Result<TypeId, &'static str> {
        let tpe = self.tpd.tpe(arg);
        let Type::Sym(prefix, c) = *self.u.types().get(tpe) else { return Ok(tpe) };
        let projection = prefix.is_some_and(|p| matches!(self.u.types().get(p), Type::Applied(..)));
        if projection && self.type_arity(tpe) > 0 {
            return Err("type projection constructor argument");
        }
        if self.u.symbol(c).kind != Kind::Class || self.u.type_params(c).is_empty() {
            return Ok(tpe);
        }
        if self.is_java_class_name(c) {
            return Err("Java class type constructor argument");
        }
        let lambda = self.eta_expanded(c, tpe)?;
        let node = self.tpd.get_mut(arg);
        (node.tpe, node.sym) = (Some(lambda), None);
        Ok(lambda)
    }

    /// How many type parameters the `i`th type parameter of class constructor `tycon` takes;
    /// `None` while that parameter is not completed.
    fn param_arity(&mut self, tycon: TypeId, i: usize) -> Option<usize> {
        let Type::Sym(_, c) = *self.u.types().get(tycon) else { return None };
        if self.u.symbol(c).kind != Kind::Class {
            return None;
        }
        let p = *self.u.type_params(c).get(i)?;
        if self.syms.completions.get(&p).is_some_and(|c| !matches!(c, Completion::Done)) {
            return None;
        }
        let info = self.u.info(p);
        Some(self.bounds_of(info).2)
    }
}
