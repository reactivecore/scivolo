// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/transform/Bridges.scala
//   compiler/src/dotty/tools/dotc/transform/ExpandPrivate.scala
//! What dotty's `Bridges` and `ExpandPrivate` give a library class javalib's build compiles in
//! the same run as the unit: its bridges and the expanded names of its private members.
#![allow(non_upper_case_globals)]

use super::bridges;
use super::mixin::{forwarded, is_jdk, mixins, with_java_param_names};
use crate::Lower;
use crate::definitions::symbol;
use crate::symbols::has;
use rustc_hash::FxHashSet;
use tasty::tags::*;
use tasty::{Ast, Kind, NodeId, SymbolId};

/// The bridges and mixin forwarders of class `class` read from TASTy and compiled in the same
/// run, each with the symbol whose span it has, made on first use; its private members its file
/// refers to from another class become public under their expanded names.
pub fn bridges(l: &mut Lower, class: SymbolId) -> Vec<(SymbolId, SymbolId)> {
    if !l.u.is_same_run(class) || l.u.is_trait(class) {
        return Vec::new();
    }
    if let Some(made) = l.library_bridges.get(&class) {
        return made.clone();
    }
    for s in referenced_privates(l, class) {
        super::accessor_names::ensure_not_private(l, s);
    }
    let mut made = Vec::new();
    let none = FxHashSet::default();
    let mut erasures = Vec::new();
    for (member, other) in bridges::overriding_pairs(l, class, &none) {
        if bridges::is_deferred(l, &none, member) || !bridges::differ(l, member, other) {
            continue;
        }
        let key = (member, l.u.erased_method(other));
        if erasures.contains(&key) {
            continue;
        }
        erasures.push(key);
        let mut info = l.u.info(other);
        if l.u.symbol(other).owner.is_some_and(|o| is_jdk(l, o)) {
            info = with_java_param_names(l, info);
        }
        let mut flags = l.u.symbol(member).flags;
        for tag in [FIELDaccessor, PARAMsetter, CASEaccessor, LAZY, OBJECT] {
            flags = flags.without(tag);
        }
        let mut s = symbol(&l.u.symbol(other).name.clone(), Kind::Def, class, flags);
        s.target = l.u.symbol(other).target.clone();
        let sym = l.u.define(s);
        l.u.set_info(sym, info);
        l.bridges.insert(sym);
        let at = if l.u.symbol(member).owner == Some(class) { member } else { class };
        made.push((sym, at));
    }
    made.extend(mixin_forwarders(l, class));
    l.library_bridges.insert(class, made.clone());
    made
}

/// The mixin forwarders dotty's `Mixin` gives library class `class`, each with `class`, whose
/// span it has.
fn mixin_forwarders(l: &mut Lower, class: SymbolId) -> Vec<(SymbolId, SymbolId)> {
    let mut made = Vec::new();
    for mixin in mixins(l, class) {
        for meth in forwarded(l, class, mixin) {
            let flags = l.u.symbol(meth).flags;
            let mut s = symbol(&l.u.symbol(meth).name.clone(), Kind::Def, class, flags);
            s.target = l.u.symbol(meth).target.clone();
            let info = l.u.info(meth);
            let sym = l.u.define(s);
            l.u.set_info(sym, info);
            made.push((sym, class));
        }
    }
    made
}

/// The private term members of `class` that its TASTy file refers to from another class.
fn referenced_privates(l: &mut Lower, class: SymbolId) -> Vec<SymbolId> {
    let Some((file, def)) = l.u.symbol(class).tree else { return Vec::new() };
    let decls = l.u.decls(class).unwrap_or_default();
    let privates: Vec<(SymbolId, NodeId)> = (decls.into_iter())
        .filter(|&s| has(l.u, s, PRIVATE) && l.u.symbol(s).kind.is_term() && !l.u.is_constructor(s))
        .filter_map(|s| Some((s, l.u.symbol(s).tree.filter(|t| t.0 == file)?.1)))
        .collect();
    if privates.is_empty() {
        return Vec::new();
    }
    let (ast, names) = l.u.file(file);
    let scan = Scan { ast, names, class: def, privates: &privates };
    let mut found = Vec::new();
    for &r in ast.roots() {
        scan.walk(r, None, &mut found);
    }
    privates.iter().map(|&(s, _)| s).filter(|s| found.contains(s)).collect()
}

struct Scan<'a> {
    ast: &'a Ast,
    names: &'a [String],
    class: NodeId,
    privates: &'a [(SymbolId, NodeId)],
}

impl Scan<'_> {
    /// Collects into `found` the privates referred to under `id` from a class other than
    /// `self.class`, `enclosing` the innermost class definition around `id`.
    fn walk(&self, id: NodeId, enclosing: Option<NodeId>, found: &mut Vec<SymbolId>) {
        let ast = self.ast;
        let n = ast.node(id);
        let enclosing = match n.tag {
            TYPEDEF if ast.kids(id).first().is_some_and(|&k| ast.tag(k) == TEMPLATE) => Some(id),
            _ => enclosing,
        };
        if enclosing.is_some_and(|c| c != self.class) {
            let target = match n.tag {
                TERMREFdirect | TERMREFsymbol => ast.at(n.a as u32),
                SELECT | SELECTin => self.selected(id),
                _ => None,
            };
            if let Some(&(s, _)) = target.and_then(|t| self.privates.iter().find(|p| p.1 == t)) {
                found.push(s);
            }
        }
        for &k in ast.kids(id) {
            self.walk(k, enclosing, found);
        }
    }

    /// The private definition selection `id` selects: one of its name from a qualifier that is
    /// `this` of the class or a value declared of its type.
    fn selected(&self, id: NodeId) -> Option<NodeId> {
        let name = self.names.get(self.ast.node(id).a as usize)?;
        let def = self
            .privates
            .iter()
            .find(|&&(_, d)| self.names.get(self.ast.node(d).a as usize) == Some(name))?
            .1;
        let qual = self.ast.deref(*self.ast.kids(id).first()?);
        let ty = match self.ast.tag(qual) {
            THIS | QUALTHIS => qual,
            IDENT => self.ast.deref(*self.ast.kids(qual).first()?),
            _ => qual,
        };
        let ty = match self.ast.tag(ty) {
            TERMREFdirect | TERMREFsymbol => {
                let value = self.ast.at(self.ast.node(ty).a as u32)?;
                matches!(self.ast.tag(value), VALDEF | PARAM)
                    .then(|| self.ast.kids(value).first().copied())??
            }
            _ => ty,
        };
        (self.class_ref(ty) == Some(self.class)).then_some(def)
    }

    /// The class definition the first type reference in type or type tree `id` names.
    fn class_ref(&self, id: NodeId) -> Option<NodeId> {
        let id = self.ast.deref(id);
        let n = self.ast.node(id);
        match n.tag {
            TYPEREFdirect | TYPEREFsymbol => self.ast.at(n.a as u32),
            THIS | QUALTHIS | IDENTtpt | APPLIEDtpt | APPLIEDtype => {
                self.class_ref(*self.ast.kids(id).first()?)
            }
            _ => None,
        }
    }
}
