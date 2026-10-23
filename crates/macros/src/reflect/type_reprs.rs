// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/scala/quoted/runtime/impl/QuotesImpl.scala
//! The shapes of `TypeRepr`s for their type tests and extractors, and the type operations over
//! them: a type's kind (1 `TypeRef`, 2 `TermRef`, 3 `AppliedType`, 4 `AnnotatedType`, 0 other),
//! its parts and its name.

use super::{Answer, UNKNOWN, arg, numbers, symbol, text};
use crate::host::Host;
use tasty::{Kind, SymbolId, Type, TypeId};

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
                    Type::Sym(Some(prefix), _) => vec![prefix],
                    Type::Sym(None, s) => vec![self.qualifier(s)?],
                    Type::TypeRef(prefix, _) | Type::TermRef(prefix, _) => vec![prefix],
                    _ => return Err("the parts of a type of another kind"),
                };
                numbers(parts.into_iter().map(|p| self.add_type(p) as i32).collect())
            }
            "showType" => {
                let t = tpe(self, 0)?;
                text(self.u.show_source(t)?)
            }
            "showTypeTree" => {
                let t = self.tree(arg(args, 0)? as u32)?;
                let inferred = matches!(self.tpd.tree(t), typer::tpd::Tree::TypeTree { .. });
                let tpe = self.tpd.get(t).tpe.filter(|&tpe| {
                    inferred && !matches!(self.u.types().get(tpe), Type::Bounds(..) | Type::Alias(_))
                });
                let tpe = tpe.ok_or("Printer.TreeCode of a tree other than an inferred type tree")?;
                text(self.u.show_source(tpe)?)
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

    /// The qualifier of a reference to `s` without a prefix: its package, the `this` of its
    /// static owner, or no prefix for a local; `Err` for a member of another class.
    fn qualifier(&mut self, s: SymbolId) -> Result<TypeId, &'static str> {
        let t = match self.u.symbol(s).owner {
            Some(o) if self.u.symbol(o).kind == Kind::Package => Type::Package(o),
            Some(o) if self.u.is_class(o) && self.u.is_static_symbol(s) => {
                Type::This(self.u.intern_type(Type::Sym(None, o)))
            }
            Some(o) if self.u.is_class(o) => {
                return Err("the qualifier of a class member without its prefix");
            }
            _ => Type::NoPrefix,
        };
        Ok(self.u.intern_type(t))
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
