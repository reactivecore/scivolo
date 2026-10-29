// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/scala/quoted/runtime/impl/QuotesImpl.scala
//! The classes macros make: `Symbol.newClass`, `ClassDef.apply`, `TypeIdent` and `New`.

use super::Fallible;
use crate::host::Host;
use tasty::{Kind, SymbolId, TypeId};
use typer::tpd::TNodeId;

impl Host {
    /// `owner`, the splice's owner for the macro's.
    pub fn made_owner(&mut self, owner: SymbolId) -> SymbolId {
        if owner == self.macro_owner() { self.owner } else { owner }
    }

    pub fn new_class(&mut self, owner: SymbolId, name: String) -> SymbolId {
        let owner = self.made_owner(owner);
        self.reflect().new_class(owner, &name)
    }

    /// The module and the module class of a new object.
    pub fn new_module(&mut self, owner: SymbolId, name: String) -> [SymbolId; 2] {
        let owner = self.made_owner(owner);
        let (module, class) = self.reflect().new_module(owner, &name);
        [module, class]
    }

    pub fn class_parents(&mut self, cls: SymbolId, parents: Vec<TypeId>) -> Fallible<()> {
        self.reflect().class_parents(cls, parents)
    }

    /// Enters the parameters of types `types` and names `names`, separated by `\0`, into `cls`.
    pub fn class_params(&mut self, cls: SymbolId, types: Vec<TypeId>, names: String) -> Fallible<()> {
        let count = types.len();
        let names = names.split('\0').filter(|n| !n.is_empty()).map(str::to_owned);
        let params: Vec<_> = names.zip(types).collect();
        if params.len() != count {
            return Err("Symbol.newClass with another number of parameter names than types");
        }
        let defs = self.reflect().class_params(cls, params)?;
        let ctor = self.u.decl(cls, "<init>").ok().and_then(|c| c.first().copied());
        self.made_params.insert(ctor.ok_or("a class without a constructor")?, vec![defs]);
        Ok(())
    }

    pub fn enter_decls(&mut self, cls: SymbolId, decls: Vec<SymbolId>) -> Fallible<()> {
        self.reflect().enter_decls(cls, &decls)
    }

    /// The definition of class `cls` with the first `n` of `trees` as its parents, the others as
    /// its body.
    pub fn class_def(&mut self, cls: SymbolId, n: i32, trees: Vec<TNodeId>) -> Fallible<TNodeId> {
        let (parents, body) =
            trees.split_at_checked(n as usize).ok_or("ClassDef.apply without its parents")?;
        let ctor = self.u.decl(cls, "<init>").ok().and_then(|c| c.first().copied());
        let params = ctor.and_then(|c| self.made_params.get(&c)).and_then(|p| p.first().cloned());
        let params = params.ok_or("ClassDef.apply of a class Symbol.newClass did not make")?;
        self.reflect().class_def(cls, (&params, parents), body)
    }

    pub fn type_ident(&mut self, s: SymbolId) -> Fallible<TNodeId> {
        self.reflect().type_ident(s)
    }

    pub fn this(&mut self, cls: SymbolId) -> Fallible<TNodeId> {
        if !self.u.is_class(cls) {
            return Err("This.apply of a symbol other than a class");
        }
        let (tpe, span) = (self.reflect().symbol_ref(cls), self.expansion);
        Ok(self.reflect().build()?.this(tpe, span))
    }

    pub fn field_member(&mut self, cls: SymbolId, name: String) -> Option<SymbolId> {
        let fields = match self.u.is_class(cls) {
            true => self.u.member_where(cls, &name, |u, m| u.symbol(m).kind == Kind::Val),
            false => Vec::new(),
        };
        fields.first().copied()
    }

    /// The declaration of `cls` that overrides method `m`.
    pub fn overriding_symbol(&mut self, m: SymbolId, cls: SymbolId) -> Option<SymbolId> {
        let name = self.u.symbol(m).name.clone();
        let sig = self.u.method_signature(m);
        let decls = self.u.decl(cls, &name).unwrap_or_default();
        decls.into_iter().find(|&d| d != m && sig.is_some() && self.u.method_signature(d) == sig)
    }

    pub fn new_(&mut self, tpt: TNodeId) -> TNodeId {
        self.reflect().new_(tpt)
    }
}
