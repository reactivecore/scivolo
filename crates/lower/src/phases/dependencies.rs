// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/SymUtils.scala
//   compiler/src/dotty/tools/dotc/transform/Dependencies.scala
//! dotty's `Dependencies` (`CollectDependencies`, `markFree`, `markCalled`, `computeFreeVars`):
//! the local values free in each local function, local class and local class constructor of a
//! unit, the owners in the order they first got one, each one's values in order. Field
//! initializers and statements of a class body count as its constructor's, as after
//! `Constructors`.

use super::explicit_outer::in_term;
use super::local_free::{is_method, is_term};
use super::mixin::{forwarded, mixins};
use super::walk::{children, subtrees};
use crate::Lower;
use crate::symbols::has;
use rustc_hash::FxHashMap;
use tasty::tags::LAZY;
use tasty::{Kind, SymbolId};
use typer::tpd::{TNodeId, Tree};

/// The free values of each owner, the owners in the order they first got one.
pub struct Dependencies {
    pub free: Vec<(SymbolId, Vec<SymbolId>)>,
}

impl Dependencies {
    /// The values free in `owner`, in order.
    pub fn of(&self, owner: SymbolId) -> &[SymbolId] {
        self.free.iter().find(|(o, _)| *o == owner).map_or(&[], |(_, vs)| vs)
    }

    /// The index of `owner` among the owners and of `value` among its values.
    pub fn rank(&self, owner: SymbolId, value: SymbolId) -> Option<(usize, usize)> {
        let i = self.free.iter().position(|(o, _)| *o == owner)?;
        Some((i, self.free[i].1.iter().position(|&v| v == value)?))
    }
}

struct Collect<'a, 'l> {
    l: &'a Lower<'l>,
    ctors: FxHashMap<SymbolId, SymbolId>,
    created: FxHashMap<TNodeId, SymbolId>,
    free: Vec<(SymbolId, Vec<SymbolId>)>,
    called: Vec<(SymbolId, Vec<SymbolId>)>,
    changed: bool,
}

/// The dependencies of unit `root`.
pub fn dependencies(l: &mut Lower, root: TNodeId) -> Dependencies {
    let (mut ctors, mut created) = (FxHashMap::default(), FxHashMap::default());
    for t in subtrees(l.tpd, root) {
        match *l.tpd.tree(t) {
            Tree::TypeDef { rhs, .. } => {
                let Tree::Template { constr, .. } = *l.tpd.tree(rhs) else { continue };
                if let (Some(c), Some(k)) = (l.tpd.sym(t), l.tpd.sym(constr)) {
                    ctors.insert(c, k);
                }
            }
            Tree::New { tpt } => {
                let tpe = l.tpd.tpe(tpt);
                created.extend(l.u.class_symbol(tpe).map(|c| (t, c)));
            }
            _ => {}
        }
    }
    let local_trait =
        |l: &Lower, m: SymbolId| l.u.is_trait(m) && l.u.symbol(m).owner.is_some_and(|o| is_term(l, o));
    let classes: Vec<SymbolId> =
        ctors.keys().copied().filter(|&c| !l.u.is_trait(c) && in_term(l, c)).collect();
    for class in classes {
        let traits: Vec<SymbolId> = mixins(l, class).into_iter().filter(|&m| local_trait(l, m)).collect();
        let methods: Vec<SymbolId> = traits.into_iter().flat_map(|m| forwarded(l, class, m)).collect();
        if !methods.is_empty() {
            l.trait_forwards.insert(class, methods);
        }
    }
    let l: &Lower = l;
    let (free, called) = (Vec::new(), Vec::new());
    let mut c = Collect { l, ctors, created, free, called, changed: false };
    c.visit(root, None);
    loop {
        c.changed = false;
        for i in 0..c.called.len() {
            let (caller, callees) = c.called[i].clone();
            for callee in callees {
                let values = c.free.iter().find(|(o, _)| *o == callee).map(|(_, vs)| vs.clone());
                for v in values.unwrap_or_default() {
                    c.mark_free(v, Some(caller));
                }
            }
        }
        if !c.changed {
            return Dependencies { free: c.free };
        }
    }
}

/// Statements `stats` with the definition of each object's class after that of its companion
/// class if that follows it (as dotty's `Namer` enters them).
fn companions_last(l: &Lower, stats: &[TNodeId]) -> Vec<TNodeId> {
    let mut out = stats.to_vec();
    for &s in stats {
        let Some(m) = l.tpd.sym(s).filter(|&m| l.u.is_module_class(m)) else { continue };
        let Some(name) = tasty::module_name(&l.u.symbol(m).name) else { continue };
        let from = out.iter().position(|&t| t == s).expect("a statement");
        let companion = out[from + 1..].iter().position(|&t| {
            let c = l.tpd.sym(t).filter(|_| matches!(l.tpd.tree(t), Tree::TypeDef { .. }));
            c.is_some_and(|c| !l.u.is_module_class(c) && l.u.symbol(c).name == name)
        });
        if let Some(k) = companion {
            out.remove(from);
            out.insert(from + k + 1, s);
        }
    }
    out
}

impl Collect<'_, '_> {
    fn visit(&mut self, t: TNodeId, encl: Option<SymbolId>) {
        let l = self.l;
        let sym = l.tpd.sym(t);
        match *l.tpd.tree(t) {
            Tree::TypeDef { rhs, .. } => {
                if let (Tree::Template { parents, body, .. }, Some(class)) = (l.tpd.tree(rhs), sym) {
                    self.template(class, l.tpd.items(*parents).to_vec(), l.tpd.items(*body).to_vec());
                }
                return;
            }
            _ if super::walk::is_type_tree(l, t) => return,
            Tree::Block { stats, expr } => {
                for s in companions_last(l, l.tpd.items(stats)) {
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
            Tree::DefDef { .. } => is_method(l, l.tpd.sym(t)),
            Tree::TypeDef { .. } => true,
            Tree::ValDef { .. } => l.tpd.sym(t).is_some_and(|s| has(l.u, s, LAZY)),
            _ => false,
        };
        for &p in parents.iter().filter(|&&p| l.tpd.sym(p).is_none_or(|s| !l.u.is_class(s))) {
            self.visit(p, ctor);
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

    /// Whether `s` is defined in a term, a method of such a trait, or the constructor of such a
    /// class (dotty's `isLocal`).
    fn is_local(&self, s: SymbolId) -> bool {
        let (l, Some(owner)) = (self.l, self.l.u.symbol(s).owner) else { return false };
        let trait_method = l.u.is_trait(owner) && l.u.symbol(s).kind == Kind::Def;
        is_term(l, owner) || (trait_method || l.u.is_constructor(s)) && self.is_local(owner)
    }

    fn add_called(&mut self, caller: Option<SymbolId>, callee: SymbolId) {
        let Some(caller) = caller else { return };
        match self.called.iter_mut().find(|(c, _)| *c == caller) {
            Some((_, cs)) if !cs.contains(&callee) => cs.push(callee),
            Some(_) => {}
            None => self.called.push((caller, vec![callee])),
        }
    }

    /// The closest method or class `s` is or is in, a class's constructor for a field or its
    /// getter (dotty's `enclosingMethodOrClass`).
    fn method_or_class(&self, mut s: SymbolId) -> Option<SymbolId> {
        let l = self.l;
        loop {
            let sym = l.u.symbol(s);
            let owner = sym.owner;
            let field = owner.is_some_and(|o| l.u.is_class(o)) && !has(l.u, s, LAZY);
            match sym.kind {
                Kind::Package => return None,
                Kind::Class => return Some(s),
                Kind::Def if l.u.is_constructor(s) || is_method(l, Some(s)) => return Some(s),
                Kind::Def | Kind::Val if field => return owner.and_then(|o| self.ctors.get(&o).copied()),
                Kind::Val if has(l.u, s, LAZY) => return Some(s),
                _ => s = owner?,
            }
        }
    }

    /// The closest method or class properly enclosing `s` (dotty's `enclosure`).
    fn enclosure(&self, s: SymbolId) -> Option<SymbolId> {
        self.method_or_class(self.l.u.symbol(s).owner?)
    }

    fn is_real_class(&self, s: Option<SymbolId>) -> bool {
        s.is_some_and(|s| self.l.u.is_class(s) && !self.l.u.is_trait(s))
    }

    fn nested_in_constructor(&self, s: SymbolId) -> bool {
        self.l.u.is_constructor(s)
            || is_term(self.l, s) && self.enclosure(s).is_some_and(|e| self.nested_in_constructor(e))
    }

    /// Marks `v` free in `encl` unless defined there or free in a class between them; the
    /// largest class between them, else `encl` if a class (dotty's `markFree`).
    fn mark_free(&mut self, v: SymbolId, encl: Option<SymbolId>) -> Option<SymbolId> {
        let encl = encl?;
        if self.enclosure(v) == Some(encl) {
            return None;
        }
        let outer = match self.l.u.is_constructor(encl) {
            true => self.l.u.symbol(encl).owner.and_then(|c| self.enclosure(c)),
            false => self.enclosure(encl),
        };
        let intermediate = self.mark_free(v, outer);
        if (!self.is_real_class(intermediate) || self.nested_in_constructor(encl)) && !self.l.u.is_trait(encl)
        {
            match self.free.iter_mut().find(|(o, _)| *o == encl) {
                Some((_, vs)) if vs.contains(&v) => {}
                Some((_, vs)) => {
                    vs.push(v);
                    self.changed = true;
                }
                None => {
                    self.free.push((encl, vec![v]));
                    self.changed = true;
                }
            }
        }
        let class = |s: Option<SymbolId>| s.is_some_and(|s| self.l.u.is_class(s));
        [intermediate, Some(encl)]
            .into_iter()
            .find(|&s| self.is_real_class(s))
            .or_else(|| [intermediate, Some(encl)].into_iter().find(|&s| class(s)))
            .flatten()
    }
}
