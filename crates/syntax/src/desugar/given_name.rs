//! Names of anonymous givens, made from their types (dotty's `inventGivenName`).

use super::Desugar;
use crate::ast::children::Child;
use crate::ast::{Ast, NodeId, TName, Tree};
use crate::names::Names;

impl Desugar<'_> {
    /// `given_` followed by the name of type `tpt`, or of the parents of template `tpt`.
    pub fn invent_given_name(&mut self, tpt: NodeId) -> TName {
        let names = TypeNames { ast: self.ast, names: self.names };
        let text = match *self.ast.tree(tpt) {
            Tree::Template { parents_or_derived, derived, .. } => {
                let items = self.ast.items(parents_or_derived);
                let parents = &items[..items.len() - derived.unwrap_or(0) as usize];
                if parents.is_empty() {
                    self.error(
                        tpt,
                        "anonymous instance must implement a type or have at least one extension method",
                    );
                    "<error>".to_string()
                } else {
                    let parents: Vec<String> = parents.iter().map(|&p| names.of(p, true)).collect();
                    format!("given_{}", parents.join("_"))
                }
            }
            _ => given_type_name(self.ast, self.names, tpt),
        };
        self.term(&text)
    }
}

/// `given_` followed by the name of type `tpt` (dotty's `inventGivenName` of a type tree).
pub fn given_type_name(ast: &Ast, names: &Names, tpt: NodeId) -> String {
    format!("given_{}", TypeNames { ast, names }.of(tpt, true))
}

struct TypeNames<'a> {
    ast: &'a Ast,
    names: &'a Names,
}

impl TypeNames<'_> {
    fn text(&self, name: TName) -> &str {
        self.names.text(name.name)
    }

    /// The name standing for type `t`: with its arguments if `follow_args`.
    fn of(&self, t: NodeId, follow_args: bool) -> String {
        let name = |n: NodeId| match *self.ast.tree(n) {
            Tree::Ident { name } => self.text(name).to_string(),
            _ => String::new(),
        };
        match *self.ast.tree(t) {
            Tree::Select { qualifier, name } if !name.is_type && self.text(name) == "<init>" => {
                self.first_child_name(qualifier, follow_args)
            }
            Tree::Ident { name } | Tree::Select { name, .. } if name.is_type => self.text(name).to_string(),
            Tree::Ident { name } | Tree::Select { name, .. } => format!("{}_type", self.text(name)),
            Tree::TypeDef { name, .. } => self.text(name).to_string(),
            Tree::AppliedTypeTree { tpt, args } if follow_args && !args.is_empty() => {
                format!("{}_{}", self.of(tpt, follow_args), self.arg_names(self.ast.items(args)))
            }
            Tree::ContextBoundTypeTree { tycon, param_name, .. } => {
                format!("{}_{}", self.of(tycon, follow_args), self.text(param_name))
            }
            Tree::InfixOp { left, op, right } if follow_args => {
                format!("{}_{}", name(op), self.arg_names(&[left, right]))
            }
            Tree::InfixOp { op, .. } => name(op),
            Tree::LambdaTypeTree { body, .. } => self.of(body, follow_args),
            Tree::Tuple { trees } => self.arg_names(self.ast.items(trees)),
            Tree::Function { args, body, .. } if !args.is_empty() => {
                if follow_args {
                    format!("{}_to_{}", self.arg_names(self.ast.items(args)), self.of(body, follow_args))
                } else {
                    "Function".into()
                }
            }
            _ => self.first_child_name(t, follow_args),
        }
    }

    fn arg_names(&self, args: &[NodeId]) -> String {
        args.iter().map(|&a| self.of(a, false)).collect::<Vec<_>>().join("_")
    }

    /// The first non-empty name among the children of `t` (dotty's `foldOver`).
    fn first_child_name(&self, t: NodeId, follow_args: bool) -> String {
        let mut trees = Vec::new();
        for &c in self.ast.children(t).iter() {
            match c {
                Child::Node(n) => trees.push(n),
                Child::List(l) => trees.extend_from_slice(self.ast.items(l)),
                Child::Clauses(cs) => {
                    for &l in self.ast.clause_items(cs) {
                        trees.extend_from_slice(self.ast.items(l));
                    }
                }
                Child::Empty | Child::Mods(_) => {}
            }
        }
        trees.iter().map(|&n| self.of(n, follow_args)).find(|s| !s.is_empty()).unwrap_or_default()
    }
}
