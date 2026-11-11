// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Implicits.scala
//! The dictionary of an outermost implicit search's by-name implicits: a local class with a value
//! per implicit, and its instance (dotty's `SearchRoot.emitDictionary`).

use super::super::Typer;
use super::history::Lazy;
use crate::defn::type_ref;
use crate::tpd::{TNodeId, Tpd, Tree as T};
use names::nme;
use rustc_hash::{FxHashMap, FxHashSet};
use syntax::ast::flags as f;
use syntax::ast::{Span, TName};
use tasty::{Kind, SymbolId, Type};

/// The emission of the by-name implicits an outermost search linked, as for
/// `given Codec[List[A]] = ...` that needs itself by name: the local class
/// `$_lazy_implicit_$N` with a value per implicit, and its instance (dotty's `emitDictionary`).
pub struct Dictionary {
    /// The by-name implicits, taken from `Typer::history`.
    lazies: Vec<Lazy>,
    /// Where the definitions are placed.
    at: Span,
    /// The owner of the definitions.
    owner: SymbolId,
}

impl Dictionary {
    /// Takes the by-name implicits linked so far out of the history, for a dictionary at `at`
    /// owned by `owner`.
    pub fn take(t: &mut Typer<'_>, at: Span, owner: SymbolId) -> Self {
        Dictionary { lazies: std::mem::take(&mut t.history.lazies), at, owner }
    }

    /// Wraps `tree`, the result of the search, in a block with the dictionary of the by-name
    /// implicits it refers to, if any; fails if one of them was never defined.
    pub fn around(self, t: &mut Typer<'_>, tree: TNodeId) -> Result<TNodeId, &'static str> {
        let (at, owner) = (self.at, self.owner);
        let pruned = self.pruned(&t.tpd, tree);
        if pruned.is_empty() {
            return Ok(tree);
        }
        let rhss: Option<Vec<TNodeId>> = pruned.iter().map(|&i| self.lazies[i].rhs).collect();
        let rhss = rhss.ok_or("no implicit argument")?;
        let (cls, ctor) = self.class(t)?;
        let mut members = FxHashMap::default();
        let mut vals = Vec::new();
        for &i in &pruned {
            let Lazy { tpe, number, ref refs, .. } = self.lazies[i];
            let m = t.syms.define(t.u, &format!("$_lazy_implicit_${number}"), Kind::Val, cls, f::EMPTY);
            t.u.set_info(m, tpe);
            t.syms.closure_syms.push(m);
            members.extend(refs.iter().map(|&r| (r, m)));
            vals.push((m, tpe));
        }
        let cls_type = type_ref(t.u, cls);
        let mut body = Vec::new();
        for ((m, tpe), rhs) in vals.into_iter().zip(rhss) {
            replace_refs(t, rhs, &members, |t, span| t.build().this(cls_type, span));
            t.change_owners(rhs, owner, m);
            let tpt = t.type_tree(tpe, at, false);
            body.push(t.build().val_def(m, tpt, Some(rhs), at));
        }
        t.history.names += 1;
        let inst_name = format!("$_lazy_implicit_${}", t.history.names);
        let inst = t.syms.define(t.u, &inst_name, Kind::Val, owner, f::EMPTY);
        t.u.set_info(inst, cls_type);
        let inst_ref = t.u.intern_type(Type::Sym(None, inst));
        replace_refs(t, tree, &members, |t, span| t.build().ident(inst, inst_ref, span));
        let class_def = self.class_def(t, cls, ctor, &body)?;
        let tpt = t.type_tree(cls_type, at, false);
        let new = self.new_call(t, cls)?;
        let inst_def = t.build().val_def(inst, tpt, Some(new), at);
        let stats = t.tpd.list(&[class_def, inst_def]);
        let result_tpe = match members.get(&tree) {
            Some(&m) => t.u.info(m),
            None => t.tpd.tpe(tree),
        };
        let block = t.build().block(stats, tree, at);
        t.tpd.get_mut(block).tpe = Some(result_tpe);
        Ok(block)
    }

    /// The by-name implicits `tree` refers to, directly or through their definitions, those found
    /// last first, those of one round in the order they were linked (dotty's `prune`, which
    /// takes that order from a hash map).
    fn pruned(&self, tpd: &Tpd, tree: TNodeId) -> Vec<usize> {
        let lazies = &self.lazies;
        let mut trees = vec![tree];
        let mut pending: Vec<usize> = (0..lazies.len()).collect();
        let mut acc = Vec::new();
        loop {
            let mut reached = FxHashSet::default();
            let mut todo = trees.clone();
            while let Some(t) = todo.pop() {
                if reached.insert(t) {
                    todo.extend(tpd.children(t));
                }
            }
            let (found, rest): (Vec<usize>, Vec<usize>) =
                pending.into_iter().partition(|&i| lazies[i].refs.iter().any(|r| reached.contains(r)));
            if found.is_empty() {
                return acc;
            }
            trees.extend(found.iter().filter_map(|&i| lazies[i].rhs));
            acc.splice(0..0, found);
            pending = rest;
        }
    }

    /// Defines the final synthetic class `$_lazy_implicit_$N`, extending `Object` and
    /// `Serializable`, and its constructor.
    fn class(&self, t: &mut Typer<'_>) -> Result<(SymbolId, SymbolId), &'static str> {
        let serializable = t.resolve_full_name("java.io.Serializable", true).ok_or("Serializable")?;
        t.history.names += 1;
        let name = format!("$_lazy_implicit_${}", t.history.names);
        let cls = t.syms.define(t.u, &name, Kind::Class, self.owner, f::FINAL.or(f::SYNTHETIC));
        let parents = vec![type_ref(t.u, t.defn.object), type_ref(t.u, serializable)];
        t.u.set_parents(cls, parents);
        t.syms.no_inits.insert(cls, false);
        t.u.set_inits(cls);
        let ctor = t.syms.define(t.u, nme::CONSTRUCTOR, Kind::Def, cls, f::METHOD.or(f::SYNTHETIC));
        let binder = t.syms.fresh_binder();
        let result = type_ref(t.u, cls);
        let info = t.u.intern_method(binder, &[], result, false, false);
        t.u.set_info(ctor, info);
        t.syms.primary_ctors.insert(cls, ctor);
        t.syms.param_clauses.insert(ctor, vec![Vec::new()]);
        Ok((cls, ctor))
    }

    /// The definition of dictionary class `cls` with constructor `ctor` and values `body`.
    fn class_def(
        &self,
        t: &mut Typer<'_>,
        cls: SymbolId,
        ctor: SymbolId,
        body: &[TNodeId],
    ) -> Result<TNodeId, &'static str> {
        let at = self.at;
        let cls_type = type_ref(t.u, cls);
        let tpt = t.type_tree(cls_type, at, false);
        let constr = t.build().def_def(ctor, &[&[]], tpt, None, at);
        let object = self.new_call(t, t.defn.object)?;
        let parents = t.u.parents(cls);
        let serializable = t.type_tree(parents[1], at, false);
        let parents = t.tpd.list(&[object, serializable]);
        let body = t.tpd.list(body);
        let template = T::Template { constr, parents, self_: None, body };
        let template = t.tpd.node(template, at, None, Some(cls));
        let name = TName { name: t.names.intern(&t.u.symbol(cls).name.clone()), is_type: true };
        Ok(t.tpd.node(T::TypeDef { name, rhs: template }, at, None, Some(cls)))
    }

    /// `new C()` of class `class` without parameters.
    fn new_call(&self, t: &mut Typer<'_>, class: SymbolId) -> Result<TNodeId, &'static str> {
        let at = self.at;
        t.complete(class);
        let ctor =
            t.u.decl(class, nme::CONSTRUCTOR).ok().and_then(|d| d.first().copied()).ok_or("constructor")?;
        let tpe = type_ref(t.u, class);
        let tpt = t.type_tree(tpe, at, false);
        let new = t.tpd.node(T::New { tpt }, at, Some(tpe), None);
        let select = t.build().select(new, ctor, at);
        Ok(t.build().apply(select, &[], tpe, at))
    }
}

/// Makes each reference in `tree` to a by-name implicit of `members` a selection of its member
/// from the qualifier `qual` makes.
fn replace_refs<'a>(
    t: &mut Typer<'a>,
    tree: TNodeId,
    members: &FxHashMap<TNodeId, SymbolId>,
    mut qual: impl FnMut(&mut Typer<'a>, Span) -> TNodeId,
) {
    let mut todo = vec![tree];
    while let Some(n) = todo.pop() {
        if let Some(&m) = members.get(&n) {
            let span = t.tpd.span(n);
            let qualifier = qual(t, span);
            let name = TName { name: t.names.intern(&t.u.symbol(m).name), is_type: false };
            let tpe = t.u.intern_type(Type::Sym(Some(t.tpd.tpe(qualifier)), m));
            let node = t.tpd.get_mut(n);
            (node.tree, node.tpe, node.sym) = (T::Select { qualifier, name }, Some(tpe), Some(m));
        } else {
            todo.extend(t.tpd.children(n));
        }
    }
}
