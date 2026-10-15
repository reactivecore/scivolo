//! The dictionary of an outermost implicit search's by-name implicits: a local class with a value
//! per implicit, and its instance (dotty's `SearchRoot.emitDictionary`).

use super::super::Typer;
use super::history::Lazy;
use crate::ctx::Ctx;
use crate::defn::type_ref;
use crate::tpd::{TNodeId, Tree as T};
use rustc_hash::{FxHashMap, FxHashSet};
use syntax::ast::flags as f;
use syntax::ast::{Span, TName};
use tasty::{Kind, SymbolId, Type};

impl Typer<'_> {
    /// `tree`, the result of an outermost search, in a block with the dictionary of the by-name
    /// implicits it refers to, if any.
    pub fn with_dictionary(&mut self, tree: TNodeId, at: Span, ctx: Ctx) -> Result<TNodeId, &'static str> {
        let lazies = std::mem::take(&mut self.history.lazies);
        let pruned = self.pruned(tree, &lazies)?;
        if pruned.is_empty() {
            return Ok(tree);
        }
        let rhss: Option<Vec<TNodeId>> = pruned.iter().map(|&i| lazies[i].rhs).collect();
        let rhss = rhss.ok_or("no implicit argument")?;
        let (cls, ctor) = self.dictionary_class(ctx.owner)?;
        let mut members = FxHashMap::default();
        let mut vals = Vec::new();
        for &i in &pruned {
            let Lazy { tpe, number, ref refs, .. } = lazies[i];
            let m = self.syms.define(self.u, &format!("$_lazy_implicit_${number}"), Kind::Val, cls, f::EMPTY);
            self.u.set_info(m, tpe);
            self.syms.closure_syms.push(m);
            members.extend(refs.iter().map(|&r| (r, m)));
            vals.push((m, tpe));
        }
        let cls_type = type_ref(self.u, cls);
        let mut body = Vec::new();
        for ((m, tpe), rhs) in vals.into_iter().zip(rhss) {
            self.replace_refs(rhs, &members, |t, span| t.build().this(cls_type, span));
            self.change_owners(rhs, ctx.owner, m);
            let tpt = self.type_tree(tpe, at, false);
            body.push(self.build().val_def(m, tpt, Some(rhs), at));
        }
        self.history.names += 1;
        let inst_name = format!("$_lazy_implicit_${}", self.history.names);
        let inst = self.syms.define(self.u, &inst_name, Kind::Val, ctx.owner, f::EMPTY);
        self.u.set_info(inst, cls_type);
        let inst_ref = self.u.intern_type(Type::Sym(None, inst));
        self.replace_refs(tree, &members, |t, span| t.build().ident(inst, inst_ref, span));
        let class_def = self.dictionary_def(cls, ctor, &body, at)?;
        let tpt = self.type_tree(cls_type, at, false);
        let new = self.new_call(cls, at)?;
        let inst_def = self.build().val_def(inst, tpt, Some(new), at);
        let stats = self.tpd.list(&[class_def, inst_def]);
        let result_tpe = match members.get(&tree) {
            Some(&m) => self.u.info(m),
            None => self.tpd.tpe(tree),
        };
        let block = self.build().block(stats, tree, at);
        self.tpd.get_mut(block).tpe = Some(result_tpe);
        Ok(block)
    }

    /// The by-name implicits `tree` refers to, directly or through their definitions, those found
    /// last first (dotty's `prune`); `Err` if one round finds several, whose order dotty takes
    /// from a hash map.
    fn pruned(&mut self, tree: TNodeId, lazies: &[Lazy]) -> Result<Vec<usize>, &'static str> {
        let mut trees = vec![tree];
        let mut pending: Vec<usize> = (0..lazies.len()).collect();
        let mut acc = Vec::new();
        loop {
            let mut reached = FxHashSet::default();
            let mut todo = trees.clone();
            while let Some(t) = todo.pop() {
                if reached.insert(t) {
                    todo.extend(self.tpd.children(t));
                }
            }
            let (found, rest): (Vec<usize>, Vec<usize>) =
                pending.into_iter().partition(|&i| lazies[i].refs.iter().any(|r| reached.contains(r)));
            match found[..] {
                [] => return Ok(acc),
                [i] => {
                    trees.extend(lazies[i].rhs);
                    acc.insert(0, i);
                }
                _ => return Err("several by-name implicits in one round of a dictionary"),
            }
            pending = rest;
        }
    }

    /// Makes each reference in `tree` to a by-name implicit of `members` a selection of its
    /// member from the qualifier `qual` makes.
    fn replace_refs(
        &mut self,
        tree: TNodeId,
        members: &FxHashMap<TNodeId, SymbolId>,
        mut qual: impl FnMut(&mut Self, Span) -> TNodeId,
    ) {
        let mut todo = vec![tree];
        while let Some(t) = todo.pop() {
            if let Some(&m) = members.get(&t) {
                let span = self.tpd.span(t);
                let qualifier = qual(self, span);
                let name = TName { name: self.names.intern(&self.u.symbol(m).name), is_type: false };
                let tpe = self.u.intern_type(Type::Sym(Some(self.tpd.tpe(qualifier)), m));
                let n = self.tpd.get_mut(t);
                (n.tree, n.tpe, n.sym) = (T::Select { qualifier, name }, Some(tpe), Some(m));
            } else {
                todo.extend(self.tpd.children(t));
            }
        }
    }

    /// The final synthetic class `$_lazy_implicit_$N` in `owner`, extending `Object` and
    /// `Serializable`, and its constructor.
    fn dictionary_class(&mut self, owner: SymbolId) -> Result<(SymbolId, SymbolId), &'static str> {
        let serializable = self.resolve_full_name("java.io.Serializable", true).ok_or("Serializable")?;
        self.history.names += 1;
        let name = format!("$_lazy_implicit_${}", self.history.names);
        let cls = self.syms.define(self.u, &name, Kind::Class, owner, f::FINAL.or(f::SYNTHETIC));
        let parents = vec![type_ref(self.u, self.defn.object), type_ref(self.u, serializable)];
        self.u.set_parents(cls, parents);
        self.syms.no_inits.insert(cls, false);
        self.u.set_inits(cls);
        let ctor = self.syms.define(self.u, "<init>", Kind::Def, cls, f::METHOD.or(f::SYNTHETIC));
        let binder = self.syms.fresh_binder();
        let result = type_ref(self.u, cls);
        let info = Type::Method { binder, params: Vec::new(), result, implicit: false, contextual: false };
        let info = self.u.intern_type(info);
        self.u.set_info(ctor, info);
        self.syms.primary_ctors.insert(cls, ctor);
        self.syms.param_clauses.insert(ctor, vec![Vec::new()]);
        Ok((cls, ctor))
    }

    /// The definition of dictionary class `cls` with constructor `ctor` and values `body`.
    fn dictionary_def(
        &mut self,
        cls: SymbolId,
        ctor: SymbolId,
        body: &[TNodeId],
        at: Span,
    ) -> Result<TNodeId, &'static str> {
        let cls_type = type_ref(self.u, cls);
        let tpt = self.type_tree(cls_type, at, false);
        let constr = self.build().def_def(ctor, &[&[]], tpt, None, at);
        let object = self.new_call(self.defn.object, at)?;
        let parents = self.u.parents(cls);
        let serializable = self.type_tree(parents[1], at, false);
        let parents = self.tpd.list(&[object, serializable]);
        let body = self.tpd.list(body);
        let template = T::Template { constr, parents, self_: None, body };
        let template = self.tpd.node(template, at, None, Some(cls));
        let name = TName { name: self.names.intern(&self.u.symbol(cls).name.clone()), is_type: true };
        Ok(self.tpd.node(T::TypeDef { name, rhs: template }, at, None, Some(cls)))
    }

    /// `new C()` of class `class` without parameters.
    fn new_call(&mut self, class: SymbolId, at: Span) -> Result<TNodeId, &'static str> {
        self.complete(class);
        let ctor = self.u.decl(class, "<init>").ok().and_then(|d| d.first().copied()).ok_or("constructor")?;
        let tpe = type_ref(self.u, class);
        let tpt = self.type_tree(tpe, at, false);
        let new = self.tpd.node(T::New { tpt }, at, Some(tpe), None);
        let select = self.build().select(new, ctor, at);
        Ok(self.build().apply(select, &[], tpe, at))
    }
}
