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
    /// The class whose `this` is replaced, with the type replacing it.
    pub this: Option<(SymbolId, TypeId)>,
}

/// What replaces a tree instead of its copy, if anything (dotty's `treeMap`).
pub type Hook<'h, 'a> = dyn FnMut(&mut Inlining<'a>, &TreeTypeMap, TNodeId) -> Option<TNodeId> + 'h;

impl<'a> Inlining<'a> {
    /// `t` with the references of `m` replaced and its local definitions renamed.
    pub fn map_type(&mut self, m: &TreeTypeMap, t: TypeId) -> TypeId {
        self.u.map_type(t, &mut |u, part| match *u.types().get(part) {
            Type::Sym(None, s) => match m.refs.get(&s) {
                Some(&r) => Some(r),
                None => m.syms.get(&s).map(|&n| u.intern_type(Type::Sym(None, n))),
            },
            Type::This(c) => m.this.filter(|&(cls, _)| u.class_symbol(c) == Some(cls)).map(|(_, to)| to),
            _ => None,
        })
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
                for s in self.tpd.items(stats).to_vec() {
                    if let (T::ValDef { .. } | T::DefDef { .. }, Some(d)) =
                        (self.tpd.tree(s), self.tpd.sym(s))
                    {
                        self.fresh_symbol(m, d);
                    }
                }
            }
            T::TypeDef { .. } if sym.is_some_and(|s| self.u.symbol(s).kind == Kind::TypeParam) => {
                self.fresh_symbol(m, sym.expect("a type parameter"));
            }
            T::ValDef { .. } | T::DefDef { .. } | T::Bind { .. } => {
                if let Some(s) = sym {
                    self.fresh_symbol(m, s);
                }
            }
            T::TypeDef { .. } | T::Template { .. } | T::Import { .. } => {
                return Err((span, "inline body with a local type, class or import"));
            }
            _ => {}
        }
        let tpe = tpe.map(|t| self.map_type(m, t));
        let sym = sym.map(|s| m.syms.get(&s).copied().unwrap_or(s));
        let (tree, call) = match tree {
            T::Inlined { call, bindings, expansion } => {
                (T::Inlined { call: None, bindings, expansion }, call)
            }
            tree => (tree, None),
        };
        let copy = self.tpd.node(tree, span, tpe, sym);
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
        self.fold_constants(copy);
        Ok(copy)
    }

    /// The fresh symbol of local definition `s` in `m`, made if it is not yet: owned by what
    /// replaces its owner, of its type mapped.
    fn fresh_symbol(&mut self, m: &mut TreeTypeMap, s: SymbolId) -> SymbolId {
        if let Some(&n) = m.syms.get(&s) {
            return n;
        }
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
        let info = self.u.info(s);
        let info = self.map_type(m, info);
        self.u.set_info(n, info);
        n
    }
}
