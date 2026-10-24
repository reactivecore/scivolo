// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Typer.scala
//! The kinds of type arguments: type constructors where higher-kinded parameters are, class
//! constructors eta-expanded (dotty's `adaptType`).

use super::Typer;
use crate::symbols::Completion;
use crate::tpd::TNodeId;
use syntax::ast::{NodeId, Tree};
use tasty::{Kind, SymbolId, Type, TypeId};

impl Typer<'_> {
    /// What about type arguments `args` of type constructor `tycon` taking `arity` is outside
    /// the subset: their number, `AnyKind` for a type other than a class, a constructor where a
    /// parameter of another kind is, or a class constructor where the parameters are unknown.
    pub fn type_args_problem(
        &mut self,
        tycon: TypeId,
        arity: usize,
        args: &[TypeId],
    ) -> Option<&'static str> {
        if arity != args.len() {
            return Some("wrong number of type arguments");
        }
        let class_tycon =
            matches!(*self.u.types().get(tycon), Type::Sym(_, s) if self.u.symbol(s).kind == Kind::Class);
        for (i, &a) in args.iter().enumerate() {
            if !class_tycon && self.u.class_symbol(a) == Some(self.u.builtins().any_kind) {
                return Some("AnyKind argument of a type alias or abstract type");
            }
            let n = self.type_arity(a);
            let wildcard = matches!(self.u.types().get(a), Type::Bounds(..));
            let class =
                matches!(*self.u.types().get(a), Type::Sym(_, s) if self.u.symbol(s).kind == Kind::Class);
            let param_arity = self.param_arity(tycon, i);
            match param_arity {
                Some(m) if m > 0 && wildcard => {
                    return Some("wildcard argument of a higher-kinded type parameter");
                }
                Some(m) if m != n => return Some("type argument of another kind"),
                _ if n > 0 && (param_arity.is_none() || !class_tycon) && self.has_bounded_params(a) => {
                    return Some("type lambda argument with bounded parameters");
                }
                None if n > 0 && class => return Some("class type constructor argument"),
                _ => {}
            }
        }
        None
    }

    /// `WrongNumberOfTypeArgs` for applied type `id` of type constructor `tycon` to `args`, if
    /// `tycon` takes type parameters and dotty shows it and the arguments as `show_user` does.
    pub fn wrong_type_arg_count(&mut self, id: NodeId, tycon: TypeId, args: &[NodeId]) -> Option<TNodeId> {
        let dealiased = self.u.dealias(tycon);
        let params: Vec<String> = match self.u.types().get(dealiased).clone() {
            Type::Sym(_, c) if self.u.full_name(c) == "scala.throws" => return None,
            Type::Sym(_, c) if self.u.symbol(c).kind == Kind::Class => {
                self.u.type_params(c).iter().map(|&p| self.u.symbol(p).name.clone()).collect()
            }
            Type::Lambda { params, .. } => params.into_iter().map(|(n, _)| n).collect(),
            _ if self.type_arity(tycon) == 0 => Vec::new(),
            _ => return None,
        };
        let mut actual = Vec::new();
        for &a in args {
            let Tree::Ident { name } = *self.ast.tree(a) else { return None };
            actual.push(self.text(name.name).to_owned());
        }
        let plain = |n: &String| !n.is_empty() && n.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
        let param = match *self.u.types().get(tycon) {
            Type::Sym(_, s) if matches!(self.u.symbol(s).kind, Kind::Type | Kind::TypeParam) => {
                let info = self.u.info(s);
                matches!(self.u.types().get(info), Type::Bounds(..))
                    && self.u.show_user(tycon) == self.u.symbol(s).name
            }
            _ => false,
        };
        let parts = self.type_parts(tycon);
        if !params.iter().all(plain)
            || !param
                && !parts
                    .iter()
                    .all(|&p| super::not_a_member::is_concrete_part(self.u, p) && self.shown_as_dotty(p))
        {
            return None;
        }
        let shown = self.u.show_user(tycon);
        if shown.contains(['<', '?', '[', ' ']) {
            return None;
        }
        let msg = match params.is_empty() {
            true => super::errors::type_does_not_take_parameters(&shown),
            false => super::errors::wrong_number_of_type_args(&shown, &params, &actual),
        };
        Some(self.error(id, msg))
    }

    /// Whether type constructor `t` is a type lambda with a parameter bounded by more than `Any`.
    fn has_bounded_params(&mut self, t: TypeId) -> bool {
        let lambda = self.u.dealias(t);
        let Type::Lambda { params, .. } = self.u.types().get(lambda).clone() else { return false };
        params.iter().any(|&(_, b)| match *self.u.types().get(b) {
            Type::Bounds(_, hi) => hi != tasty::ANY && self.u.class_symbol(hi) != Some(self.u.builtins().any),
            _ => false,
        })
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

    /// How many type parameters the `i`th type parameter of class or type lambda constructor
    /// `tycon` takes; while a class's parameter is not completed, the number of type parameters
    /// of its tree in the class's primary constructor if it has any, else `None`.
    fn param_arity(&mut self, tycon: TypeId, i: usize) -> Option<usize> {
        let Type::Sym(_, c) = *self.u.types().get(tycon) else { return None };
        if matches!(self.u.symbol(c).kind, Kind::Type | Kind::TypeParam) {
            self.complete(c);
            let info = self.u.info(c);
            let lambda = match *self.u.types().get(info) {
                Type::Bounds(_, hi) => hi,
                _ => info,
            };
            let Type::Lambda { params, .. } = self.u.types().get(lambda).clone() else { return None };
            return Some(self.bounds_of(params.get(i)?.1).2);
        }
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
