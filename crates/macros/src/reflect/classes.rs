// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/scala/quoted/runtime/impl/QuotesImpl.scala
//! The classes macros make: `Symbol.newClass`, `ClassDef.apply`, `TypeIdent` and `New`.

use super::{Answer, UNKNOWN, arg, numbers, symbol};
use crate::host::Host;
use tasty::Kind;

impl Host {
    /// Runs class-making operation `op` on `args` and `name`.
    pub fn run_class(&mut self, op: &str, args: &[i32], name: &str) -> Answer {
        let rest = |from: usize| args.get(from..).unwrap_or_default();
        match op {
            "newClass" => {
                let owner = match symbol(arg(args, 0)?)? {
                    o if o == self.macro_owner() => self.owner,
                    o => o,
                };
                let c = self.reflect().new_class(owner, name);
                numbers(vec![c.0 as i32])
            }
            "classParents" | "classParams" => {
                let cls = symbol(arg(args, 0)?)?;
                let types = rest(1).iter().map(|&p| self.type_of(p as u32)).collect::<Result<Vec<_>, _>>()?;
                if op == "classParents" {
                    self.reflect().class_parents(cls, types)?;
                    return numbers(Vec::new());
                }
                let names = name.split('\0').filter(|n| !n.is_empty()).map(str::to_owned);
                let params: Vec<_> = names.zip(types).collect();
                if params.len() != rest(1).len() {
                    return Err("Symbol.newClass with another number of parameter names than types");
                }
                let defs = self.reflect().class_params(cls, params)?;
                let ctor = self.u.decl(cls, "<init>").ok().and_then(|c| c.first().copied());
                self.made_params.insert(ctor.ok_or("a class without a constructor")?, vec![defs]);
                numbers(Vec::new())
            }
            "enterDecls" => {
                let decls = rest(1).iter().map(|&d| symbol(d)).collect::<Result<Vec<_>, _>>()?;
                self.reflect().enter_decls(symbol(arg(args, 0)?)?, &decls)?;
                numbers(Vec::new())
            }
            "classDef" => {
                let (cls, n) = (symbol(arg(args, 0)?)?, arg(args, 1)? as usize);
                let trees = rest(2).iter().map(|&t| self.tree(t as u32)).collect::<Result<Vec<_>, _>>()?;
                let (parents, body) =
                    trees.split_at_checked(n).ok_or("ClassDef.apply without its parents")?;
                let ctor = self.u.decl(cls, "<init>").ok().and_then(|c| c.first().copied());
                let params = ctor.and_then(|c| self.made_params.get(&c)).and_then(|p| p.first().cloned());
                let params = params.ok_or("ClassDef.apply of a class Symbol.newClass did not make")?;
                let t = self.reflect().class_def(cls, (&params, parents), body)?;
                numbers(vec![self.add_tree(t) as i32])
            }
            "typeIdent" => {
                let t = self.reflect().type_ident(symbol(arg(args, 0)?)?)?;
                numbers(vec![self.add_tree(t) as i32])
            }
            "this" => {
                let cls = symbol(arg(args, 0)?)?;
                if !self.u.is_class(cls) {
                    return Err("This.apply of a symbol other than a class");
                }
                let (tpe, span) = (self.reflect().symbol_ref(cls), self.expansion);
                let t = self.reflect().build()?.this(tpe, span);
                numbers(vec![self.add_tree(t) as i32])
            }
            "fieldMember" => {
                let cls = symbol(arg(args, 0)?)?;
                let fields = match self.u.is_class(cls) {
                    true => self.u.member_where(cls, name, |u, m| u.symbol(m).kind == Kind::Val),
                    false => Vec::new(),
                };
                numbers(vec![fields.first().map_or(-1, |f| f.0 as i32)])
            }
            "overridingSymbol" => {
                let (m, cls) = (symbol(arg(args, 0)?)?, symbol(arg(args, 1)?)?);
                let name = self.u.symbol(m).name.clone();
                let sig = self.u.method_signature(m);
                let decls = self.u.decl(cls, &name).unwrap_or_default();
                let found =
                    decls.into_iter().find(|&d| d != m && sig.is_some() && self.u.method_signature(d) == sig);
                numbers(vec![found.map_or(-1, |f| f.0 as i32)])
            }
            "new" => {
                let tpt = self.tree(arg(args, 0)? as u32)?;
                let t = self.reflect().new_(tpt);
                numbers(vec![self.add_tree(t) as i32])
            }
            _ => Err(UNKNOWN),
        }
    }
}
