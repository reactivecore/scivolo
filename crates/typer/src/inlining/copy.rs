// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/ast/TreeInfo.scala
//   compiler/src/dotty/tools/dotc/ast/TreeTypeMap.scala
//   compiler/src/dotty/tools/dotc/core/Symbols.scala
//! Copies of typed trees with their types mapped and their local definitions given fresh symbols
//! (dotty's `TreeTypeMap`).

use super::{Inlining, Unsupported};
use crate::tpd::{TNodeId, Tree as T, map_children};
use rustc_hash::FxHashMap;
use tasty::{Kind, Symbol, SymbolId, Type, TypeId};

#[derive(Default)]
pub struct TreeTypeMap {
    /// The references replaced, by the symbol referred to, with the types replacing them.
    pub refs: FxHashMap<SymbolId, TypeId>,
    /// The local definitions copied, from old symbol to new.
    pub syms: FxHashMap<SymbolId, SymbolId>,
    /// The owner whose definitions get `new_owner` as owner when copied.
    pub old_owner: Option<SymbolId>,
    pub new_owner: Option<SymbolId>,
    /// The classes whose `this` is replaced, with the types replacing them.
    pub this: Vec<(SymbolId, TypeId)>,
    /// Whether classes are among the local definitions copied.
    pub classes: bool,
}

/// What replaces a tree instead of its copy, if anything (dotty's `treeMap`).
pub type Hook<'h, 'a> = dyn FnMut(&mut Inlining<'a>, &TreeTypeMap, TNodeId) -> Option<TNodeId> + 'h;

impl<'a> Inlining<'a> {
    /// `t` with the references of `m` replaced and its local definitions renamed.
    pub fn map_type(&mut self, m: &TreeTypeMap, t: TypeId) -> TypeId {
        let mapped = self.u.map_type(t, &mut |u, part| {
            if !m.this.is_empty()
                && matches!(u.types().get(part), Type::Sym(..))
                && let Some(seen) = super::outer_this::class_param_seen(u, &m.this, part)
            {
                return Some(seen);
            }
            match *u.types().get(part) {
                Type::Sym(None, s) => match m.refs.get(&s) {
                    Some(&r) => Some(r),
                    None => m.syms.get(&s).map(|&n| u.intern_type(Type::Sym(None, n))),
                },
                Type::This(c) => {
                    let c = u.class_symbol(c);
                    m.this.iter().find(|&&(cls, _)| c == Some(cls)).map(|&(_, to)| to)
                }
                _ => None,
            }
        });
        let mapped = if mapped == t { t } else { self.reselected_members(mapped) };
        match m.classes {
            true => super::copy_classes::rename_members(self.u, &m.syms, mapped),
            false => mapped,
        }
    }

    /// `t` with each abstract type member selected from a path selecting instead the member of
    /// that name the path's type defines, if it defines a concrete one (dotty's name-based
    /// `TypeRef`s).
    fn reselected_members(&mut self, t: TypeId) -> TypeId {
        self.u.map_type(t, &mut |u, part| {
            let Type::Sym(Some(p), s) = *u.types().get(part) else { return None };
            if u.symbol(s).kind != Kind::Type
                || !matches!(u.types().get(p), Type::Sym(..))
                || u.alias_of(s).is_some()
            {
                return None;
            }
            let name = u.symbol(s).name.clone();
            let r = u.resolve(p, &name, true).filter(|&r| r != s && u.alias_of(r).is_some())?;
            Some(u.intern_type(Type::Sym(Some(p), r)))
        })
    }

    /// A copy of `t` with its local definitions given fresh symbols.
    pub fn fresh_copy(&mut self, t: TNodeId) -> Result<TNodeId, Unsupported> {
        self.copy(&mut TreeTypeMap::default(), t, &mut |_, _, _| None)
    }

    /// A copy of `t` through `m`, each tree first offered to `hook`.
    pub fn copy(
        &mut self,
        m: &mut TreeTypeMap,
        t: TNodeId,
        hook: &mut Hook<'_, 'a>,
    ) -> Result<TNodeId, Unsupported> {
        if let Some(r) = hook(self, m, t) {
            return Ok(r);
        }
        let node = self.tpd.get(t);
        let (tree, span, tpe, sym) = (node.tree.clone(), node.span, node.tpe, node.sym);
        match tree {
            T::Block { stats, .. } => {
                let defs: Vec<SymbolId> = self
                    .tpd
                    .items(stats)
                    .iter()
                    .filter(|&&s| {
                        matches!(self.tpd.tree(s), T::ValDef { .. } | T::DefDef { .. } | T::TypeDef { .. })
                    })
                    .filter_map(|&s| self.tpd.sym(s))
                    .collect();
                self.fresh_symbols(m, &defs);
            }
            T::CaseDef { pat, .. } => {
                let vars = self.pattern_vars(pat);
                self.fresh_symbols(m, &vars);
            }
            T::TypeDef { .. } if sym.is_some_and(|s| self.u.symbol(s).kind == Kind::TypeParam) => {
                self.fresh_symbol(m, sym.expect("a type parameter"));
            }
            T::RefinedTypeTree { .. } => {
                let tpe = tpe.map(|t| self.map_type(m, t));
                return Ok(self.tpd.node(T::TypeTree { inferred: false }, span, tpe, None));
            }
            T::ValDef { .. } if sym.is_some_and(|s| self.u.is_module(s)) => {
                return Err((span, "inline body with a local object"));
            }
            T::ValDef { .. } | T::DefDef { .. } | T::Bind { .. } | T::TypeDef { .. } => {
                if let Some(s) = sym {
                    self.fresh_symbol(m, s);
                }
            }
            T::Import { .. } => return Err((span, "inline body with an import")),
            _ => {}
        }
        let tpe = tpe.map(|t| self.map_type(m, t));
        let sym = sym.map(|s| m.syms.get(&s).copied().unwrap_or(s));
        let (tree, call) = match self.without_imports(tree) {
            T::Inlined { call, bindings, expansion } => {
                (T::Inlined { call: None, bindings, expansion }, call)
            }
            tree => (tree, None),
        };
        let is_class_def =
            matches!(tree, T::TypeDef { rhs, .. } if matches!(self.tpd.tree(rhs), T::Template { .. }));
        let copy = self.tpd.node(tree, span, tpe, sym);
        self.host.copied_call(t, copy);
        let mut failed = None;
        map_children(self, copy, &mut |i, c| match i.copy(m, c, hook) {
            Ok(n) => n,
            Err(e) => {
                failed.get_or_insert(e);
                c
            }
        });
        if let Some(e) = failed {
            return Err(e);
        }
        if let T::Inlined { call: c, .. } = &mut self.tpd.get_mut(copy).tree {
            *c = call;
        }
        if is_class_def {
            self.note_inits(copy);
        }
        self.fold_constants(copy);
        Ok(copy)
    }

    /// The symbols the binds of pattern `pat` define, outermost first (dotty's `patVars`).
    fn pattern_vars(&self, pat: TNodeId) -> Vec<SymbolId> {
        let own = match self.tpd.tree(pat) {
            T::Bind { .. } => self.tpd.sym(pat),
            _ => None,
        };
        let inner = self.tpd.children(pat).into_iter().flat_map(|c| self.pattern_vars(c));
        own.into_iter().chain(inner).collect()
    }

    /// The fresh symbol of local definition `s` in `m`, made if it is not yet: owned by what
    /// replaces its owner, of its type mapped.
    fn fresh_symbol(&mut self, m: &mut TreeTypeMap, s: SymbolId) {
        self.fresh_symbols(m, &[s]);
    }

    /// Fresh symbols of local definitions `syms` not yet in `m`, all made before their types are
    /// mapped (dotty's `mapSymbols`).
    fn fresh_symbols(&mut self, m: &mut TreeTypeMap, syms: &[SymbolId]) {
        let mut made = Vec::new();
        for &s in syms {
            if !m.syms.contains_key(&s) {
                self.declared_fresh_with_members(m, s, &mut made);
            }
        }
        for (s, n) in made {
            self.complete_fresh(m, s, n);
        }
    }

    /// A fresh symbol of `s` recorded in `m`, its type not yet set.
    pub fn declared_fresh(&mut self, m: &mut TreeTypeMap, s: SymbolId) -> SymbolId {
        let old = self.u.symbol(s);
        let owner = old.owner.map(|o| match m.syms.get(&o) {
            Some(&n) => n,
            None if Some(o) == m.old_owner => m.new_owner.unwrap_or(o),
            None => o,
        });
        let fresh = Symbol {
            name: old.name.clone(),
            kind: old.kind,
            owner,
            flags: old.flags,
            target: None,
            is_extern: false,
            derived: old.derived,
            decls: Vec::new(),
            tree: None,
        };
        let n = self.u.define(fresh);
        m.syms.insert(s, n);
        n
    }
}
