//! The kinds of type arguments: type constructors where higher-kinded parameters are, class
//! constructors eta-expanded (dotty's `adaptType`).

use super::Typer;
use crate::symbols::Completion;
use crate::tpd::TNodeId;
use syntax::ast::{NodeId, Tree};
use tasty::{Kind, SymbolId, Type, TypeId};

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
            let wildcard = matches!(self.u.types().get(a), Type::Bounds(..));
            let class =
                matches!(*self.u.types().get(a), Type::Sym(_, s) if self.u.symbol(s).kind == Kind::Class);
            match self.param_arity(tycon, i) {
                Some(m) if m > 0 && wildcard => {
                    return Some("wildcard argument of a higher-kinded type parameter");
                }
                Some(m) if m != n => return Some("type argument of another kind"),
                None if n > 0 && class => return Some("class type constructor argument"),
                _ => {}
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
    /// while that parameter is not completed, the number of type parameters of its tree in the
    /// class's primary constructor if it has any, else `None`.
    fn param_arity(&mut self, tycon: TypeId, i: usize) -> Option<usize> {
        let Type::Sym(_, c) = *self.u.types().get(tycon) else { return None };
        if self.u.symbol(c).kind != Kind::Class {
            return None;
        }
        let p = *self.u.type_params(c).get(i)?;
        if self.syms.completions.get(&p).is_some_and(|c| !matches!(c, Completion::Done)) {
            return self.param_tree_arity(c, i);
        }
        let info = self.u.info(p);
        Some(self.bounds_of(info).2)
    }

    /// The number of type parameters of the `i`th type parameter tree of script class `c`'s
    /// primary constructor; `None` if it has none.
    fn param_tree_arity(&self, c: SymbolId, i: usize) -> Option<usize> {
        let Tree::TypeDef { rhs, .. } = *self.ast.tree(*self.syms.def_trees.get(&c)?) else { return None };
        let Tree::Template { constr, .. } = *self.ast.tree(rhs) else { return None };
        let Tree::DefDef { paramss, .. } = *self.ast.tree(constr) else { return None };
        let is_type_def = |&p: &NodeId| matches!(self.ast.tree(p), Tree::TypeDef { .. });
        let mut clauses = self.ast.clause_items(paramss).iter().map(|&l| self.ast.items(l));
        let clause = clauses.find(|ps| ps.first().is_some_and(is_type_def))?;
        let Tree::TypeDef { rhs, .. } = *self.ast.tree(*clause.get(i)?) else { return None };
        let Tree::LambdaTypeTree { tparams, .. } = *self.ast.tree(rhs) else { return None };
        Some(self.ast.items(tparams).len())
    }
}
