// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/scala/quoted/runtime/impl/QuotesImpl.scala
//! The definitions macros make: new value and method symbols, fresh names, `ValDef`s, `DefDef`s
//! and copies of definitions.

use super::Fallible;
use crate::host::Host;
use syntax::ast::TName;
use tasty::tags::CASE;
use tasty::{Kind, SymbolId, TypeId};
use typer::tpd::{TNodeId, Tree as T};

impl Host {
    pub fn fresh_name(&mut self, name: String) -> String {
        let fresh = self.fresh.next(&mut self.names, &format!("{name}$macro$"));
        self.names.text(fresh).to_owned()
    }

    pub fn new_val(&mut self, owner: SymbolId, info: TypeId, flags: u64, name: String) -> Fallible<SymbolId> {
        let owner = self.made_owner(owner);
        self.reflect().new_term((owner, &name, Kind::Val), info, flags)
    }

    pub fn new_method(
        &mut self,
        owner: SymbolId,
        info: TypeId,
        flags: u64,
        name: String,
    ) -> Fallible<SymbolId> {
        let owner = self.made_owner(owner);
        self.reflect().new_term((owner, &name, Kind::Def), info, flags)
    }

    /// `Symbol.newBind(owner, name, flags, tpe)`: a pattern variable, `Case` added to `flags`.
    pub fn new_bind(&mut self, owner: SymbolId, info: TypeId, flags: u64, name: String) -> SymbolId {
        let owner = self.made_owner(owner);
        let flags = tasty::Flags(flags & !(1 << super::symbols::METHOD)).with(CASE);
        let s = super::symbols::local(&mut self.u, &name, Kind::Val, owner, flags);
        self.u.set_info(s, info);
        s
    }

    /// `Bind(s, pattern)`, of the reference to `s`.
    pub fn bind(&mut self, s: SymbolId, pattern: TNodeId) -> Fallible<TNodeId> {
        if !self.u.symbol(s).flags.has(CASE) || self.u.symbol(s).kind != Kind::Val {
            return Err("Bind.apply of a symbol other than a pattern variable");
        }
        let name = TName { name: self.names.intern(&self.u.symbol(s).name.clone()), is_type: false };
        let tpe = self.u.intern_type(tasty::Type::Sym(None, s));
        Ok(self.tpd.node(T::Bind { name, body: pattern }, self.expansion, Some(tpe), Some(s)))
    }

    pub fn val_def(&mut self, s: SymbolId, rhs: Option<TNodeId>) -> Fallible<TNodeId> {
        self.reflect().val_def(s, rhs)
    }

    /// The number of clauses of method `s`, their sizes and references to the parameters.
    pub fn def_params(&mut self, s: SymbolId) -> Fallible<Vec<i32>> {
        let params = self.method_params(s)?;
        let mut answer = vec![params.len() as i32];
        answer.extend(params.iter().map(|c| c.len() as i32));
        for p in params.into_iter().flatten() {
            let s = self.tpd.sym(p).ok_or("a parameter without a symbol")?;
            let r = self.reflect().ref_(s)?;
            answer.push(self.add_tree(r) as i32);
        }
        Ok(answer)
    }

    pub fn def_def(&mut self, s: SymbolId, rhs: Option<TNodeId>) -> Fallible<TNodeId> {
        let params = self.method_params(s)?;
        self.reflect().def_def(s, &params, rhs)
    }

    pub fn closure(&mut self, def: TNodeId) -> Fallible<TNodeId> {
        self.reflect().closure(def)
    }

    pub fn lambda_of(&mut self, t: TNodeId) -> Vec<TNodeId> {
        self.lambda_parts_of(t).unwrap_or_default()
    }

    /// A copy of definition `original` named `name` with parameters `params`.
    pub fn copy_def(
        &mut self,
        original: TNodeId,
        tpt: TNodeId,
        rhs: Option<TNodeId>,
        params: Vec<TNodeId>,
        name: String,
    ) -> Fallible<TNodeId> {
        self.reflect().copy_def(original, (&name, &params), tpt, rhs)
    }

    /// The parameters and body of `t` if it is `{ def f(params) = body; closure(f) }` (dotty's
    /// `Lambda.unapply`), the body last.
    fn lambda_parts_of(&self, t: TNodeId) -> Option<Vec<TNodeId>> {
        let T::Block { stats, expr } = *self.tpd.tree(t) else { return None };
        let (&[def], T::Closure { meth, .. }) = (self.tpd.items(stats), self.tpd.tree(expr)) else {
            return None;
        };
        let T::DefDef { paramss, rhs: Some(body), .. } = *self.tpd.tree(def) else { return None };
        let &[params] = self.tpd.clause_items(paramss) else { return None };
        let params = self.tpd.items(params);
        let values = params.iter().all(|&p| matches!(self.tpd.tree(p), T::ValDef { .. }));
        (values && self.tpd.sym(*meth) == self.tpd.sym(def)).then(|| [params, &[body]].concat())
    }

    /// The parameter definitions of method `s` by clause: those of its definition in the unit,
    /// else ones made once from its type.
    pub fn method_params(&mut self, s: SymbolId) -> Result<Vec<Vec<TNodeId>>, &'static str> {
        if let Some(params) = self.made_params.get(&s) {
            return Ok(params.clone());
        }
        let params = match self.definition(s).map(|d| self.tpd.tree(d).clone()) {
            Some(T::DefDef { paramss, .. }) => {
                self.tpd.clause_items(paramss).iter().map(|&l| self.tpd.items(l).to_vec()).collect()
            }
            _ => self.reflect().method_params(s)?,
        };
        if params.iter().flatten().any(|&p| matches!(self.tpd.tree(p), T::TypeDef { .. })) {
            return Err("DefDef.apply of a polymorphic method");
        }
        self.made_params.insert(s, params.clone());
        Ok(params)
    }
}
