// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/scala/quoted/runtime/impl/QuotesImpl.scala
//! The shapes of `TypeRepr`s for their type tests and extractors, and the type operations over
//! them: a type's kind (1 `TypeRef`, 2 `TermRef`, 3 `AppliedType`, 4 `AnnotatedType`, 0 other),
//! its parts and its name.

use super::{Answer, UNKNOWN, arg, numbers, symbol, text};
use crate::host::Host;
use tasty::{Kind, Type, TypeId};

impl Host {
    /// Runs type operation `op` on `args`.
    pub fn run_type_repr(&mut self, op: &str, args: &[i32], _: &str) -> Answer {
        let tpe = |h: &Host, i: usize| h.type_of(arg(args, i)? as u32);
        match op {
            "typeKind" => {
                let t = tpe(self, 0)?;
                numbers(vec![self.type_kind(t)])
            }
            "typeParts" => {
                let t = tpe(self, 0)?;
                let parts = match self.u.types().get(t).clone() {
                    Type::Applied(tycon, args) => [tycon].into_iter().chain(args).collect(),
                    Type::Annotated(inner, _) => vec![inner],
                    Type::Sym(prefix, s) => {
                        let owner =
                            self.u.symbol(s).owner.filter(|&o| self.u.symbol(o).kind == Kind::Package);
                        let qualifier =
                            prefix.or_else(|| owner.map(|o| self.u.intern_type(Type::Package(o))));
                        vec![qualifier.unwrap_or_else(|| self.u.intern_type(Type::NoPrefix))]
                    }
                    Type::TypeRef(prefix, _) | Type::TermRef(prefix, _) => vec![prefix],
                    _ => return Err("the parts of a type of another kind"),
                };
                numbers(parts.into_iter().map(|p| self.add_type(p) as i32).collect())
            }
            "typeName" => {
                let t = tpe(self, 0)?;
                match self.u.types().get(t).clone() {
                    Type::Sym(_, s) => text(self.u.symbol(s).name.clone()),
                    Type::TypeRef(_, n) | Type::TermRef(_, n) => text(n),
                    _ => Err("the name of an unnamed type"),
                }
            }
            "appliedTo" => {
                let tycon = tpe(self, 0)?;
                let targs = (1..args.len()).map(|i| tpe(self, i)).collect::<Result<Vec<_>, _>>()?;
                let t = match targs.is_empty() {
                    true => tycon,
                    false => self.u.intern_type(Type::Applied(tycon, targs)),
                };
                numbers(vec![self.add_type(t) as i32])
            }
            "dealias" => {
                let t = tpe(self, 0)?;
                let d = self.u.dealias(t);
                numbers(vec![self.add_type(d) as i32])
            }
            "isSingleton" => {
                let t = tpe(self, 0)?;
                let singleton = match *self.u.types().get(t) {
                    Type::Sym(_, s) => self.u.symbol(s).kind.is_term(),
                    Type::TermRef(..) | Type::This(_) | Type::Super(..) | Type::Skolem(..) => true,
                    Type::Constant(..) | Type::Str(_) => true,
                    _ => false,
                };
                numbers(vec![singleton as i32])
            }
            "baseClasses" => {
                let t = tpe(self, 0)?;
                let w = self.u.widen(t);
                let c = self.u.class_symbol(w).ok_or("baseClasses of a type without a class")?;
                numbers(self.u.linearization(c).into_iter().map(|b| b.0 as i32).collect())
            }
            "memberType" => {
                let (t, s) = (tpe(self, 0)?, symbol(arg(args, 1)?)?);
                let m = self.u.term_type(Some(t), s);
                numbers(vec![self.add_type(m) as i32])
            }
            "symbolRef" => {
                let t = self.reflect().symbol_ref(symbol(arg(args, 0)?)?);
                numbers(vec![self.add_type(t) as i32])
            }
            _ => Err(UNKNOWN),
        }
    }

    fn type_kind(&self, t: TypeId) -> i32 {
        match *self.u.types().get(t) {
            Type::Sym(_, s) if self.u.symbol(s).kind.is_term() => 2,
            Type::Sym(..) | Type::TypeRef(..) => 1,
            Type::TermRef(..) => 2,
            Type::Applied(..) => 3,
            Type::Annotated(..) => 4,
            _ => 0,
        }
    }
}
