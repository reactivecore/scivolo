// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/scala/quoted/runtime/impl/QuotesImpl.scala
//! The definitions of `quotes.reflect`: the annotations and trees of symbols, the bodies of
//! classes and the clauses of methods.

use super::decode::Expr;
use super::{Answer, Fallible};
use crate::host::Host;
use tasty::SymbolId;
use typer::tpd::{TNodeId, Tree as T};

impl Host {
    /// The annotations of `s`, those of class `cls` if given.
    pub fn annotations(&mut self, s: SymbolId, cls: Option<SymbolId>) -> Fallible<Vec<TNodeId>> {
        if self.definition(s).is_none() {
            let owner = self.u.symbol(s).owner.map(|o| self.u.full_name(o));
            let universal = matches!(owner.as_deref(), Some("java.lang.Object" | "scala.Any"));
            return match cls {
                Some(_) if universal => Ok(Vec::new()),
                _ => Err("the annotations of a symbol defined outside the unit"),
            };
        }
        let mut trees = self.typer.annotations.get(&s).cloned().unwrap_or_default();
        if let Some(cls) = cls {
            trees.retain(|&t| self.u.class_symbol(self.tpd.tpe(t)) == Some(cls));
        }
        Ok(trees)
    }

    pub fn symbol_tree(&mut self, s: SymbolId) -> Fallible<Option<TNodeId>> {
        Ok(match self.definition(s) {
            None if self.settings.retain_trees && self.u.symbol(s).tree.is_some() => {
                Some(typer::unpickle::member_def(&mut self.u, &mut self.tpd, &mut self.names, s)?)
            }
            t => t,
        })
    }

    pub fn class_body(&mut self, t: TNodeId) -> Fallible<Vec<TNodeId>> {
        let T::TypeDef { rhs, .. } = *self.tpd.tree(t) else { return Err(NO_CLASS) };
        let T::Template { body, .. } = *self.tpd.tree(rhs) else { return Err(NO_CLASS) };
        Ok(self.tpd.items(body).to_vec())
    }

    /// The number of clauses of method definition `t`, whether each is of types and its size,
    /// and the parameters.
    pub fn def_clauses(&mut self, t: TNodeId) -> Fallible<Vec<i32>> {
        let T::DefDef { paramss, .. } = *self.tpd.tree(t) else {
            return Err("DefDef.paramss of another tree");
        };
        let clauses: Vec<Vec<TNodeId>> =
            self.tpd.clause_items(paramss).iter().map(|&l| self.tpd.items(l).to_vec()).collect();
        let mut answer = vec![clauses.len() as i32];
        for c in &clauses {
            let is_type = c.first().is_some_and(|&p| matches!(self.tpd.tree(p), T::TypeDef { .. }));
            answer.extend([is_type as i32, c.len() as i32]);
        }
        answer.extend(clauses.into_iter().flatten().map(|p| self.add_tree(p) as i32));
        Ok(answer)
    }

    /// The constructor, self, number of parents, parents and body of class definition `t`, and
    /// its name.
    pub fn class_parts(&mut self, t: TNodeId) -> Answer {
        let T::TypeDef { name, rhs } = *self.tpd.tree(t) else { return Err(NO_CLASS) };
        let T::Template { constr, parents, self_, body } = *self.tpd.tree(rhs) else {
            return Err(NO_CLASS);
        };
        let (parents, body) = (self.tpd.items(parents).to_vec(), self.tpd.items(body).to_vec());
        let mut answer = vec![self.add_tree(constr) as i32, self_.map_or(-1, |s| self.add_tree(s) as i32)];
        answer.push(parents.len() as i32);
        answer.extend(parents.into_iter().chain(body).map(|p| self.add_tree(p) as i32));
        Ok((answer, Some(self.names.text(name.name).to_owned())))
    }

    /// A copy of class definition `t` named `name` with the first `n` of `rest` as its parents,
    /// the others as its body.
    pub fn copy_class(
        &mut self,
        t: TNodeId,
        constr: TNodeId,
        self_: Option<TNodeId>,
        n: i32,
        rest: Vec<TNodeId>,
        name: String,
    ) -> Fallible<TNodeId> {
        let (parents, body) = rest.split_at_checked(n as usize).ok_or("ClassDef.copy without its parents")?;
        self.reflect().copy_class(t, (&name, constr, self_), parents, body)
    }

    pub fn set_definitions(&mut self, defs: Vec<Expr>) {
        self.definitions = Some(defs.into_iter().map(|d| d.number as u32).collect());
    }

    /// The definition of `s` among the typer's trees (dotty's `Symbol.tree`), if it has one.
    pub fn definition(&self, s: SymbolId) -> Option<TNodeId> {
        let defines = |d: TNodeId| {
            self.tpd.sym(d) == Some(s)
                && matches!(
                    self.tpd.tree(d),
                    T::ValDef { .. } | T::DefDef { .. } | T::TypeDef { .. } | T::Bind { .. }
                )
        };
        (0..self.tpd.len()).rev().map(TNodeId::new).find(|&d| defines(d))
    }
}

const NO_CLASS: &str = "ClassDef.body of another tree";
