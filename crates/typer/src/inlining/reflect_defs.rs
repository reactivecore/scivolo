// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/scala/quoted/runtime/impl/QuotesImpl.scala
//! The definitions macros build with `quotes.reflect`: new value symbols, their `ValDef`s and
//! copies of definitions with other right-hand sides.

use super::Reflect;
use crate::tpd::{TNodeId, Tree as T};
use tasty::tags::{FINAL, GIVEN, IMPLICIT, LAZY, LOCAL, MUTABLE, OVERRIDE, PRIVATE, PROTECTED, SYNTHETIC};
use tasty::{Flags, Kind, Symbol, SymbolId, TypeId};

impl Reflect<'_> {
    /// `Symbol.newVal(owner, name, tpe, flags, _)`, not entered into its owner's declarations.
    pub fn new_val(
        &mut self,
        owner: SymbolId,
        name: &str,
        info: TypeId,
        flags: u64,
    ) -> Result<SymbolId, &'static str> {
        let allowed = [PRIVATE, PROTECTED, FINAL, IMPLICIT, LAZY, OVERRIDE, LOCAL, SYNTHETIC, MUTABLE, GIVEN];
        if flags & !allowed.iter().fold(0, |b, &t| b | 1u64 << t) != 0 {
            return Err("Symbol.newVal with flags other than modifiers of values");
        }
        let s = self.u.define(Symbol {
            name: name.to_owned(),
            kind: Kind::Val,
            owner: None,
            flags: Flags(flags),
            target: None,
            is_extern: false,
            derived: false,
            decls: Vec::new(),
            tree: None,
        });
        self.u.set_owner(s, owner);
        self.u.set_info(s, info);
        Ok(s)
    }

    /// `ValDef(s, rhs)`.
    pub fn val_def(&mut self, s: SymbolId, rhs: Option<TNodeId>) -> Result<TNodeId, &'static str> {
        if self.u.symbol(s).kind != Kind::Val {
            return Err("ValDef.apply of a symbol other than a value");
        }
        let (span, info) = (self.span, self.u.info(s));
        let tpt = self.type_tree(info);
        Ok(self.build()?.val_def(s, tpt, rhs, span))
    }

    /// `ValDef.copy(original)(name, tpt, rhs)` and `DefDef.copy(original)(name, paramss, tpt, rhs)`
    /// with the parameters of `original`: its symbol and name kept.
    pub fn copy_def(
        &mut self,
        original: TNodeId,
        (name, params): (&str, &[TNodeId]),
        tpt: TNodeId,
        rhs: Option<TNodeId>,
    ) -> Result<TNodeId, &'static str> {
        let s = self.tpd.sym(original).ok_or("a copy of a definition without a symbol")?;
        if self.u.symbol(s).name != name {
            return Err("a copy of a definition under another name");
        }
        let tree = match *self.tpd.tree(original) {
            T::ValDef { name, .. } if params.is_empty() => T::ValDef { name, tpt, rhs },
            T::DefDef { name, paramss, .. } => {
                let old = self.tpd.clause_items(paramss).iter().flat_map(|&l| self.tpd.items(l).to_vec());
                if !old.eq(params.iter().copied()) {
                    return Err("DefDef.copy with other parameters");
                }
                T::DefDef { name, paramss, tpt, rhs }
            }
            _ => return Err("ValDef.copy or DefDef.copy of another tree"),
        };
        Ok(self.tpd.node(tree, self.tpd.span(original), None, Some(s)))
    }
}
