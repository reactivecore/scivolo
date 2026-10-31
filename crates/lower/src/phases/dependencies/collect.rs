// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/transform/Dependencies.scala
//! dotty's `CollectDependencies`: the calls and free values of a unit's trees.

use super::Collect;
use crate::phases::local_free::is_method;
use crate::phases::walk::{children, is_type_tree};
use crate::symbols::has;
use tasty::tags::LAZY;
use tasty::{Kind, SymbolId};
use typer::tpd::{TNodeId, Tree};

impl Collect<'_, '_> {
    pub fn visit(&mut self, t: TNodeId, encl: Option<SymbolId>) {
        let l = self.l;
        let sym = l.tpd.sym(t);
        match *l.tpd.tree(t) {
            Tree::TypeDef { rhs, .. } => {
                if let (Tree::Template { parents, body, .. }, Some(class)) = (l.tpd.tree(rhs), sym) {
                    self.template(class, l.tpd.items(*parents).to_vec(), l.tpd.items(*body).to_vec());
                }
                return;
            }
            _ if is_type_tree(l, t) => return,
            Tree::Block { stats, expr } => {
                for s in l.tpd.items(stats).to_vec() {
                    self.visit(s, encl);
                }
                self.visit(expr, encl);
                return;
            }
            Tree::DefDef { .. } => {
                let encl = sym.filter(|&s| is_method(l, Some(s)) || l.u.is_constructor(s)).or(encl);
                for c in children(l.tpd, t) {
                    self.visit(c, encl);
                }
                return;
            }
            Tree::Closure { meth, .. } => {
                if let Some(f) = l.tpd.sym(meth).filter(|&f| self.is_local(f)) {
                    self.called.push(((f, true), vec![f]));
                }
            }
            Tree::Ident { .. } | Tree::Select { .. } if sym.is_some_and(|s| self.is_local(s)) => {
                let s = sym.expect("a symbol");
                match l.u.symbol(s).kind {
                    Kind::Def => self.add_called(encl, s),
                    Kind::Val | Kind::Param if matches!(l.tpd.tree(t), Tree::Ident { .. }) => {
                        self.mark_free(s, encl);
                    }
                    _ => {}
                }
            }
            Tree::New { .. } => {
                let ctor = self.created.get(&t).and_then(|c| self.ctors.get(c).copied());
                if let Some(ctor) = ctor {
                    self.add_called(encl, ctor);
                }
                return;
            }
            _ => {}
        }
        for c in children(l.tpd, t) {
            self.visit(c, encl);
        }
    }

    /// The constructor of `class`, its parents and initializing statements, then its members.
    fn template(&mut self, class: SymbolId, parents: Vec<TNodeId>, body: Vec<TNodeId>) {
        let l = self.l;
        let ctor = self.ctors.get(&class).copied();
        if self.is_local(class) && !l.u.is_trait(class) {
            self.add_called(ctor, class);
        }
        for m in l.trait_forwards.get(&class).cloned().unwrap_or_default() {
            self.add_called(Some(class), m);
        }
        let member = |t: TNodeId| match *l.tpd.tree(t) {
            Tree::DefDef { .. } => {
                l.tpd.sym(t).is_some_and(|s| is_method(l, Some(s)) || l.u.is_constructor(s))
            }
            Tree::TypeDef { .. } => true,
            Tree::ValDef { .. } => l.tpd.sym(t).is_some_and(|s| has(l.u, s, LAZY)),
            _ => false,
        };
        for &p in parents.iter().filter(|&&p| l.tpd.sym(p).is_none_or(|s| !l.u.is_class(s))) {
            self.visit(p, ctor);
        }
        for init in self.inits.get(&class).cloned().unwrap_or_default() {
            self.add_called(ctor, init);
        }
        for &s in body.iter().filter(|&&s| !member(s)) {
            let rhs = match *l.tpd.tree(s) {
                Tree::ValDef { rhs, .. } | Tree::DefDef { rhs, .. } => rhs,
                _ => Some(s),
            };
            if let Some(rhs) = rhs {
                self.visit(rhs, ctor);
            }
        }
        for &s in body.iter().filter(|&&s| member(s)) {
            self.visit(s, Some(class));
        }
    }

    fn is_local(&self, s: SymbolId) -> bool {
        super::is_local(self.l, s)
    }

    fn add_called(&mut self, caller: Option<SymbolId>, callee: SymbolId) {
        let Some(caller) = caller else { return };
        match self.called.iter_mut().find(|(c, _)| *c == (caller, false)) {
            Some((_, cs)) if !cs.contains(&callee) => cs.push(callee),
            Some(_) => {}
            None => self.called.push(((caller, false), vec![callee])),
        }
    }
}
