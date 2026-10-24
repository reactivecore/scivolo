// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/scala/quoted/runtime/impl/QuotesImpl.scala
//! The definitions macros make: new value and method symbols, fresh names, `ValDef`s, `DefDef`s
//! and copies of definitions.

use super::{Answer, UNKNOWN, arg, numbers, symbol, text};
use crate::host::Host;
use tasty::{Kind, SymbolId};
use typer::tpd::{TNodeId, Tree as T};

impl Host {
    /// Runs definition-making operation `op` on `args` and `name`.
    pub fn run_made(&mut self, op: &str, args: &[i32], name: &str) -> Answer {
        let arg = |i: usize| arg(args, i);
        let tree = |h: &Host, n: i32| match n {
            -1 => Ok(None),
            n => h.tree(n as u32).map(Some),
        };
        match op {
            "freshName" => {
                let fresh = self.fresh.next(&mut self.names, &format!("{name}$macro$"));
                text(self.names.text(fresh).to_owned())
            }
            "newVal" | "newMethod" => {
                let (owner, info) = (symbol(arg(0)?)?, self.type_of(arg(1)? as u32)?);
                let owner = if owner == self.macro_owner() { self.owner } else { owner };
                let flags = (arg(2)? as u32 as u64) | ((arg(3)? as u32 as u64) << 32);
                let kind = if op == "newVal" { Kind::Val } else { Kind::Def };
                let s = self.reflect().new_term((owner, name, kind), info, flags)?;
                numbers(vec![s.0 as i32])
            }
            "valDef" => {
                let (s, rhs) = (symbol(arg(0)?)?, tree(self, arg(1)?)?);
                let t = self.reflect().val_def(s, rhs)?;
                numbers(vec![self.add_tree(t) as i32])
            }
            "defParams" => {
                let params = self.method_params(symbol(arg(0)?)?)?;
                let mut answer = vec![params.len() as i32];
                answer.extend(params.iter().map(|c| c.len() as i32));
                for p in params.into_iter().flatten() {
                    let s = self.tpd.sym(p).ok_or("a parameter without a symbol")?;
                    let r = self.reflect().ref_(s)?;
                    answer.push(self.add_tree(r) as i32);
                }
                numbers(answer)
            }
            "defDef" => {
                let (s, rhs) = (symbol(arg(0)?)?, tree(self, arg(1)?)?);
                let params = self.method_params(s)?;
                let t = self.reflect().def_def(s, &params, rhs)?;
                numbers(vec![self.add_tree(t) as i32])
            }
            "closure" => {
                let def = self.tree(arg(0)? as u32)?;
                let t = self.reflect().closure(def)?;
                numbers(vec![self.add_tree(t) as i32])
            }
            "etaExpand" => {
                let (t, owner) = (self.tree(arg(0)? as u32)?, symbol(arg(1)?)?);
                let owner = if owner == self.macro_owner() { self.owner } else { owner };
                let info = self.tpd.get(t).tpe.ok_or("Term.etaExpand of a term without a type")?;
                let info = self.u.widen(info);
                if !matches!(self.u.types().get(info), tasty::Type::Method { .. }) {
                    return numbers(vec![arg(0)?]);
                }
                let synthetic = 1 << tasty::tags::SYNTHETIC;
                let meth = self.reflect().new_term((owner, "$anonfun", Kind::Def), info, synthetic)?;
                let params = self.method_params(meth)?;
                let mut refs = Vec::new();
                for p in params.iter().flatten() {
                    let s = self.tpd.sym(*p).ok_or("a parameter without a symbol")?;
                    refs.push(self.reflect().ref_(s)?);
                }
                let body = self.reflect().apply(t, &refs)?;
                let def = self.reflect().def_def(meth, &params, Some(body))?;
                let closure = self.reflect().closure(def)?;
                numbers(vec![self.add_tree(closure) as i32])
            }
            "lambdaOf" => {
                let t = self.tree(arg(0)? as u32)?;
                let parts = self.lambda_parts(t).unwrap_or_default();
                numbers(parts.into_iter().map(|p| self.add_tree(p) as i32).collect())
            }
            "copyDef" => {
                let (original, tpt, rhs) =
                    (self.tree(arg(0)? as u32)?, self.tree(arg(1)? as u32)?, tree(self, arg(2)?)?);
                let params = args[3..].iter().map(|&p| self.tree(p as u32)).collect::<Result<Vec<_>, _>>()?;
                let t = self.reflect().copy_def(original, (name, &params), tpt, rhs)?;
                numbers(vec![self.add_tree(t) as i32])
            }
            _ => Err(UNKNOWN),
        }
    }

    /// The parameters and body of `t` if it is `{ def f(params) = body; closure(f) }` (dotty's
    /// `Lambda.unapply`), the body last.
    fn lambda_parts(&self, t: TNodeId) -> Option<Vec<TNodeId>> {
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
    fn method_params(&mut self, s: SymbolId) -> Result<Vec<Vec<TNodeId>>, &'static str> {
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
