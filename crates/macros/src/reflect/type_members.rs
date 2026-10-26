// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/scala/quoted/runtime/impl/QuotesImpl.scala
//! The type and field members of classes in `quotes.reflect`, and the objects
//! `Symbol.requiredModule` looks up.

use super::{Answer, UNKNOWN, arg, numbers, symbol};
use crate::host::Host;
use tasty::tags::PRIVATE;
use tasty::{Kind, SymbolId};

impl Host {
    /// Runs member operation `op` on `args` and `text`.
    pub fn run_type_member(&mut self, op: &str, args: &[i32], text: &str) -> Answer {
        let ids = |syms: Vec<SymbolId>| numbers(syms.into_iter().map(|s| s.0 as i32).collect());
        match op {
            "typeMembers" => {
                let s = symbol(arg(args, 0)?)?;
                ids(self.members(s, |k| k.is_type(), None)?)
            }
            "fieldMembers" => {
                let s = symbol(arg(args, 0)?)?;
                let name = (!text.is_empty()).then_some(text);
                ids(self.members(s, |k| k.is_term() && k != Kind::Def, name)?)
            }
            "isAliasType" => {
                let s = symbol(arg(args, 0)?)?;
                let alias = self.u.symbol(s).kind.is_type() && !self.u.is_class(s) && {
                    let info = self.u.info(s);
                    matches!(self.u.types().get(info), tasty::Type::Alias(_))
                };
                numbers(vec![alias as i32])
            }
            "requiredModule" => {
                let (pkg, name) = text.rsplit_once('.').unwrap_or(("", text));
                let p = self.u.package(pkg).ok_or("Symbol.requiredModule in a package the frontend lacks")?;
                let decls = self.u.decl(p, name).unwrap_or_default();
                let module = decls.into_iter().find(|&d| self.u.is_module(d)).or_else(|| {
                    let objects = self.u.decl(p, "package").unwrap_or_default();
                    let class = objects
                        .into_iter()
                        .find(|&d| self.u.is_module(d))
                        .and_then(|m| self.u.module_class(m))?;
                    let vals = self.u.decl(class, name).unwrap_or_default();
                    vals.into_iter().find(|&v| self.u.symbol(v).kind == Kind::Val)
                });
                numbers(vec![module.ok_or("Symbol.requiredModule of an object the frontend lacks")?.0 as i32])
            }
            _ => Err(UNKNOWN),
        }
    }

    /// The members of class `s` of kinds `of`, named `name` if given, in the order of dotty's
    /// `Symbol.typeMembers`: by base class in the order of `sortedParents`, the type parameters
    /// of each first in their order, then the others by name; a member is left out when one of
    /// a class earlier in the linearization has its name or it is private to another class.
    fn members(
        &mut self,
        s: SymbolId,
        of: fn(Kind) -> bool,
        name: Option<&str>,
    ) -> Result<Vec<SymbolId>, &'static str> {
        if !self.u.is_class(s) {
            return match self.u.symbol(s).kind.is_type() {
                true => Err("the members of a type that is no class"),
                false => Ok(Vec::new()),
            };
        }
        let mut found: Vec<SymbolId> = Vec::new();
        for base in self.u.linearization(s) {
            let own: Vec<SymbolId> = (self.u.symbol(base).decls.clone().into_iter())
                .filter(|&d| {
                    let d = self.u.symbol(d);
                    of(d.kind) && !d.name.starts_with("<local ") && (base == s || !d.flags.has(PRIVATE))
                })
                .filter(|&d| name.is_none_or(|n| self.u.symbol(d).name == n))
                .filter(|&d| !found.iter().any(|&f| self.u.symbol(f).name == self.u.symbol(d).name))
                .collect();
            found.extend(own);
        }
        let mut order = Vec::new();
        self.sorted_parents(s, &mut order);
        found.sort_by_key(|&d| {
            let sym = self.u.symbol(d);
            let placement = sym.owner.and_then(|o| order.iter().position(|&c| c == o)).unwrap_or(usize::MAX);
            let param = sym.kind == Kind::TypeParam;
            (placement, !param, if param { String::new() } else { sym.name.clone() })
        });
        Ok(found)
    }

    /// Class `c` and its parents' classes, each followed by theirs, each once (dotty's
    /// `sortedParents`).
    fn sorted_parents(&mut self, c: SymbolId, order: &mut Vec<SymbolId>) {
        if order.contains(&c) {
            return;
        }
        order.push(c);
        for p in self.u.parents(c) {
            if let Some(pc) = self.u.class_symbol(p) {
                self.sorted_parents(pc, order);
            }
        }
    }
}
