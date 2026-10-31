// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/transform/Dependencies.scala
//! dotty's `Dependencies` (`CollectDependencies`, `markFree`, `markCalled`, `computeFreeVars`):
//! the local values free in each local function, local class and local class constructor of a
//! unit, the owners in the order they first got one, each one's values in order. Field
//! initializers and statements of a class body count as its constructor's, as after
//! `Constructors`. Each lambda counts as called by its `$adapted` method, as after `Erasure`.

mod collect;
mod mark_free;

use super::explicit_outer::in_term;
use super::local_free::{is_term, original_owner};
use super::mixin::{forwarded, mixins};
use super::walk::subtrees;
use crate::Lower;
use rustc_hash::FxHashMap;
use tasty::{Kind, SymbolId};
use typer::tpd::{TNodeId, Tree};

/// The free values of each owner, the owners in the order they first got one.
pub struct Dependencies {
    pub free: Vec<(SymbolId, Vec<SymbolId>)>,
    /// The innermost enclosing class each local class uses the values of (dotty's `logicOwner`).
    pub logic_owners: FxHashMap<SymbolId, SymbolId>,
    /// The owners in the order they first got a value, `true` for the `$adapted` method of the
    /// lambda (the keys of dotty's `free`).
    pub order: Vec<(SymbolId, bool)>,
    /// The number of owners `CollectDependencies` found, before `computeFreeVars`.
    pub traversed: usize,
}

impl Dependencies {
    /// The values free in `owner`, in order.
    pub fn of(&self, owner: SymbolId) -> &[SymbolId] {
        self.free.iter().find(|(o, _)| *o == owner).map_or(&[], |(_, vs)| vs)
    }

    /// The index of `owner` among the owners and of `value` among its values.
    pub fn rank(&self, owner: SymbolId, value: SymbolId) -> Option<(usize, usize)> {
        let i = self.order.iter().position(|&o| o == (owner, false))?;
        Some((i, self.of(owner).iter().position(|&v| v == value)?))
    }

    /// The index of the `$adapted` method of lambda `f` among the owners, and `j`.
    pub fn adapted_rank(&self, f: SymbolId, j: usize) -> Option<(usize, usize)> {
        Some((self.order.iter().position(|&o| o == (f, true))?, j))
    }
}

struct Collect<'a, 'l> {
    l: &'a Lower<'l>,
    ctors: FxHashMap<SymbolId, SymbolId>,
    inits: FxHashMap<SymbolId, Vec<SymbolId>>,
    created: FxHashMap<TNodeId, SymbolId>,
    free: Vec<(SymbolId, Vec<SymbolId>)>,
    /// Each caller, `true` for the `$adapted` method of the lambda, and its callees.
    called: Vec<((SymbolId, bool), Vec<SymbolId>)>,
    logic_owners: FxHashMap<SymbolId, SymbolId>,
    order: Vec<(SymbolId, bool)>,
    /// The number of values of each lambda its `$adapted` method has.
    adapted: FxHashMap<SymbolId, usize>,
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
    let local_trait = |l: &Lower, m: SymbolId| l.u.is_trait(m) && is_local(l, m);
    let classes: Vec<SymbolId> =
        ctors.keys().copied().filter(|&c| !l.u.is_trait(c) && in_term(l, c)).collect();
    let mut inits = FxHashMap::default();
    for class in classes {
        let traits: Vec<SymbolId> = mixins(l, class).into_iter().filter(|&m| local_trait(l, m)).collect();
        inits.insert(class, traits.iter().filter_map(|m| ctors.get(m).copied()).collect());
        let methods: Vec<SymbolId> = traits.into_iter().flat_map(|m| forwarded(l, class, m)).collect();
        if !methods.is_empty() {
            l.trait_forwards.insert(class, methods);
        }
    }
    let l: &Lower = l;
    let (free, called, order) = (Vec::new(), Vec::new(), Vec::new());
    let (logic_owners, adapted) = (FxHashMap::default(), FxHashMap::default());
    let mut c =
        Collect { l, ctors, inits, created, free, called, logic_owners, order, adapted, changed: false };
    c.visit(root, None);
    let traversed = c.order.len();
    c.compute_free_vars();
    Dependencies { free: c.free, logic_owners: c.logic_owners, order: c.order, traversed }
}

/// Whether `s` is defined in a term, a method or class of a local trait, or is the constructor
/// of a local class (dotty's `isLocal`), before local classes were lifted.
pub fn is_local(l: &Lower, s: SymbolId) -> bool {
    let Some(owner) = original_owner(l, s) else { return false };
    let trait_member = l.u.is_trait(owner) && (l.u.symbol(s).kind == Kind::Def || l.u.is_class(s));
    is_term(l, owner) || (trait_member || l.u.is_constructor(s)) && is_local(l, owner)
}

impl Collect<'_, '_> {
    /// Marks the values of each callee free in its callers until none changes (dotty's
    /// `computeFreeVars`).
    fn compute_free_vars(&mut self) {
        loop {
            self.changed = false;
            for i in 0..self.called.len() {
                let ((caller, adapted), callees) = self.called[i].clone();
                for callee in callees {
                    let values = self.free.iter().find(|(o, _)| *o == callee).map(|(_, vs)| vs.clone());
                    let values = values.unwrap_or_default();
                    if adapted {
                        self.adapt(caller, values.len());
                        continue;
                    }
                    for v in values {
                        self.mark_free(v, Some(caller));
                    }
                }
            }
            if !self.changed {
                return;
            }
        }
    }

    /// Gives the `$adapted` method of lambda `f` the first `n` values of `f`.
    fn adapt(&mut self, f: SymbolId, n: usize) {
        let had = self.adapted.get(&f).copied();
        if n == 0 || had.is_some_and(|h| h >= n) {
            return;
        }
        if had.is_none() {
            self.order.push((f, true));
        }
        self.adapted.insert(f, n);
        self.changed = true;
    }
}
