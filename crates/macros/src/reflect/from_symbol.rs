// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/quoted/reflect/FromSymbol.scala
//   compiler/src/dotty/tools/dotc/transform/Splicer.scala
//! The macro owner, the definitions in macro arguments (which dotty's `Splicer` copies, without
//! their trees) and the definitions `Symbol.tree` makes from symbols without a tree.

use super::Fallible;
use crate::host::Host;
use names::nme;
use std::collections::HashSet;
use syntax::ast::TName;
use tasty::tags::{CASE, ENUM, MACRO, OBJECT, SYNTHETIC};
use tasty::{Flags, Kind, SymbolId, Type};
use typer::tpd::{TNodeId, Tree as T};

/// The expressions passed to the macro and the symbols defined in them, collected on first use.
#[derive(Default)]
pub struct Args {
    roots: Vec<TNodeId>,
    defined: Option<HashSet<SymbolId>>,
}

impl Host {
    /// The value `macro` of type `Any` that owns the splice (dotty's `Splicer.splice`).
    pub fn macro_owner(&mut self) -> SymbolId {
        if let Some(s) = self.made.macro_owner {
            return s;
        }
        let flags = Flags::default().with(MACRO).with(SYNTHETIC);
        let s = super::symbols::local(&mut self.u, "macro", Kind::Val, self.owner, flags);
        let any = self.u.class_ref("scala.Any");
        self.u.set_info(s, any);
        *self.made.macro_owner.insert(s)
    }

    /// Expression `t` numbered as an argument of the macro.
    pub fn add_arg(&mut self, t: TNodeId) -> u32 {
        self.made.args.roots.push(t);
        self.add_tree(t)
    }

    /// Whether `s` is defined in an argument of the macro.
    pub fn in_args(&mut self, s: SymbolId) -> bool {
        if self.made.args.defined.is_none() {
            let (mut defined, mut todo) = (HashSet::new(), self.made.args.roots.clone());
            while let Some(t) = todo.pop() {
                if matches!(
                    self.tpd.tree(t),
                    T::ValDef { .. } | T::DefDef { .. } | T::TypeDef { .. } | T::Bind { .. }
                ) {
                    defined.extend(self.tpd.sym(t));
                }
                todo.extend(self.tpd.children(t));
            }
            self.made.args.defined = Some(defined);
        }
        self.made.args.defined.as_ref().is_some_and(|d| d.contains(&s))
    }

    /// Owner `o` of `s` as dotty's `Splicer` changes it: the macro owner if `s` is defined in an
    /// argument of the macro and `o` is not.
    pub fn spliced_owner(&mut self, s: SymbolId, o: SymbolId) -> SymbolId {
        if self.in_args(s) && !self.in_args(o) { self.macro_owner() } else { o }
    }

    /// The definition of `s` made from its symbol, without right-hand sides (dotty's
    /// `definitionFromSym` of a symbol without a tree); none for a package.
    pub fn definition_from_symbol(&mut self, s: SymbolId) -> Fallible<Option<TNodeId>> {
        let sym = self.u.symbol(s);
        let (kind, flags) = (sym.kind, sym.flags);
        Ok(Some(match kind {
            Kind::Package => return Ok(None),
            Kind::Class => self.class_from_symbol(s)?,
            Kind::Type if flags.has(CASE) => {
                let (body, span) = (self.wildcard(), self.expansion);
                let name = TName { name: self.names.intern(&self.u.symbol(s).name), is_type: true };
                let tpe = self.reflect().symbol_ref(s);
                self.tpd.node(T::Bind { name, body }, span, Some(tpe), Some(s))
            }
            Kind::Type | Kind::TypeParam => self.type_def_from_symbol(s),
            Kind::Def => self.def_def(s, None)?,
            Kind::Val if flags.has(CASE) && !flags.has(OBJECT) && !flags.has(ENUM) => {
                let pattern = self.wildcard();
                self.bind(s, pattern)?
            }
            _ => {
                let info = self.u.info(s);
                let tpt = self.reflect().type_tree(info);
                let span = self.expansion;
                self.reflect().build()?.val_def(s, tpt, None, span)
            }
        }))
    }

    /// `TypeDef(name, TypeTree(info))` of type `s`, its info as bounds: an alias's both its type.
    fn type_def_from_symbol(&mut self, s: SymbolId) -> TNodeId {
        let info = self.u.info(s);
        let (lo, hi) = match *self.u.ty(info) {
            Type::Bounds(lo, hi) => (lo, hi),
            Type::Alias(t) => (t, t),
            _ => (info, info),
        };
        let bounds = self.u.intern_type(Type::Bounds(lo, hi));
        let (lo, hi) = (self.reflect().type_tree(lo), self.reflect().type_tree(hi));
        let span = self.expansion;
        let rhs = self.tpd.node(T::TypeBoundsTree { lo, hi, alias: None }, span, Some(bounds), None);
        let name = TName { name: self.names.intern(&self.u.symbol(s).name), is_type: true };
        let tpe = self.reflect().symbol_ref(s);
        self.tpd.node(T::TypeDef { name, rhs }, span, Some(tpe), Some(s))
    }

    /// The definition of class `cls` from its symbol: its constructor, parents and declarations.
    fn class_from_symbol(&mut self, cls: SymbolId) -> Fallible<TNodeId> {
        let mut decls = self.u.symbol(cls).decls.clone();
        let ctor = decls.iter().copied().find(|&d| self.u.is_constructor(d));
        let constr = ctor.unwrap_or_else(|| {
            super::symbols::local(&mut self.u, nme::CONSTRUCTOR, Kind::Def, cls, Flags::default())
        });
        let constr = self.definition_from_symbol(constr)?;
        decls.retain(|&d| Some(d) != ctor && !self.u.symbol(d).name.starts_with("<local "));
        let mut body = Vec::new();
        for d in decls {
            body.extend(self.definition_from_symbol(d)?);
        }
        let parents: Vec<TNodeId> =
            self.u.parents(cls).into_iter().map(|p| self.reflect().type_tree(p)).collect();
        let span = self.expansion;
        let (parents, body) = (self.tpd.list(&parents), self.tpd.list(&body));
        let template = T::Template { constr: constr.ok_or("a constructor")?, parents, self_: None, body };
        let rhs = self.tpd.node(template, span, None, None);
        let name = TName { name: self.names.intern(&self.u.symbol(cls).name), is_type: true };
        let tpe = self.reflect().symbol_ref(cls);
        Ok(self.tpd.node(T::TypeDef { name, rhs }, span, Some(tpe), Some(cls)))
    }
}
