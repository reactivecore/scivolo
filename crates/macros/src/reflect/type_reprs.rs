// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/scala/quoted/runtime/impl/QuotesImpl.scala
//! The shapes of `TypeRepr`s for their type tests and extractors, and the type operations over
//! them: a type's kind (1 `TypeRef`, 2 `TermRef`, 3 `AppliedType`, 4 `AnnotatedType`, 0 other),
//! its parts and its name.

use super::{Answer, UNKNOWN, arg, numbers, symbol, text};
use crate::host::Host;
use tasty::{Kind, SymbolId, Type, TypeId};
use typer::tpd::Tree as T;

impl Host {
    /// Runs type operation `op` on `args`.
    pub fn run_type_repr(&mut self, op: &str, args: &[i32], name: &str) -> Answer {
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
            "methodType" => {
                let types = args.iter().map(|&t| self.type_of(t as u32)).collect::<Result<Vec<_>, _>>()?;
                let (&result, params) =
                    types.split_last().ok_or("MethodType.apply without its result type")?;
                let names: Vec<String> =
                    name.split('\0').filter(|n| !n.is_empty()).map(str::to_owned).collect();
                if names.len() != params.len() {
                    return Err("MethodType.apply with another number of names than types");
                }
                let binder = self.u.fresh_binder();
                let params = names.into_iter().zip(params.iter().copied()).collect();
                let t = Type::Method { binder, params, result, implicit: false, contextual: false };
                let t = self.u.intern_type(t);
                numbers(vec![self.add_type(t) as i32])
            }
            "byName" => {
                let t = tpe(self, 0)?;
                let by_name = self.u.intern_type(Type::ByName(t));
                numbers(vec![self.add_type(by_name) as i32])
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
                let m = self.repeated_param(s)?.unwrap_or_else(|| self.u.term_type(Some(t), s));
                numbers(vec![self.add_type(m) as i32])
            }
            "termRef" => {
                let qual = tpe(self, 0)?;
                let t = self.reflect().term_ref(qual, name)?;
                numbers(vec![self.add_type(t) as i32])
            }
            "annotation" => {
                let t = tpe(self, 0)?;
                let Type::Annotated(_, annot) = *self.u.types().get(t) else {
                    return Err("AnnotatedType.annotation of another type");
                };
                let span = self.expansion;
                let tpt = self.tpd.node(T::TypeTree { inferred: false }, span, Some(annot), None);
                let new = self.tpd.node(T::New { tpt }, span, Some(annot), None);
                numbers(vec![self.add_tree(new) as i32])
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

    /// The type `<repeated>[T]` of `s` if it is a repeated parameter of a method, as its
    /// method's type states it (dotty's info of a repeated parameter); `Err` for one of a
    /// polymorphic method.
    fn repeated_param(&mut self, s: SymbolId) -> Result<Option<TypeId>, &'static str> {
        let sym = self.u.symbol(s);
        let (name, Some(owner)) = (sym.name.clone(), sym.owner) else { return Ok(None) };
        if sym.kind != Kind::Param || self.u.symbol(owner).kind != Kind::Def {
            return Ok(None);
        }
        let (mut t, mut poly) = (self.u.info(owner), false);
        while let Type::Poly { result, .. } | Type::Method { result, .. } = self.u.types().get(t).clone() {
            if let Type::Method { params, .. } = self.u.types().get(t).clone()
                && let Some(&(_, p)) = params.iter().find(|(n, _)| *n == name)
            {
                let repeated = self.u.class_symbol(p).is_some_and(|c| self.u.symbol(c).name == "<repeated>");
                return match (repeated, poly) {
                    (true, true) => Err("memberType of a repeated parameter of a polymorphic method"),
                    _ => Ok(repeated.then_some(p)),
                };
            }
            poly |= matches!(self.u.types().get(t), Type::Poly { .. });
            t = result;
        }
        Ok(None)
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
