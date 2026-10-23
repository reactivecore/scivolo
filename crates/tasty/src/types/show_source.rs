// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/scala/quoted/runtime/impl/printers/SourceCode.scala
//! Types as `quotes.reflect`'s `Printer.TypeReprCode` shows them (dotty's `SourceCode.printType`
//! with full names): `scala.collection.immutable.List[scala.Int]`, `scala.Predef.String`.

use super::{Binder, Type, TypeId};
use crate::view::{Const, constant};
use crate::{Kind, SymbolId, Universe};

type Shown = Result<(), &'static str>;

const REFUSED: &str = "TypeRepr.show of a type the frontend does not print";

impl Universe {
    /// Type `t` as `Printer.TypeReprCode` shows it.
    pub fn show_source(&mut self, t: TypeId) -> Result<String, &'static str> {
        let mut p = SourcePrinter { u: self, binders: Vec::new(), out: String::new() };
        p.print(t)?;
        Ok(p.out)
    }
}

struct SourcePrinter<'u> {
    u: &'u mut Universe,
    /// The parameter names of the enclosing binding types.
    binders: Vec<(Binder, Vec<String>)>,
    out: String,
}

impl SourcePrinter<'_> {
    fn print(&mut self, t: TypeId) -> Shown {
        match self.u.types().get(t).clone() {
            Type::Constant(tag, bits) => {
                let text = match constant(tag, bits) {
                    Some(Const::Int(n)) => n.to_string(),
                    Some(Const::Long(n)) => format!("{n}L"),
                    Some(Const::Bool(b)) => b.to_string(),
                    Some(Const::Null) => "null".into(),
                    Some(Const::Unit) => "()".into(),
                    _ => return Err(REFUSED),
                };
                self.out.push_str(&text);
            }
            Type::Str(s) if s.chars().all(|c| c.is_ascii_graphic() && c != '"' && c != '\\' || c == ' ') => {
                self.out.push_str(&format!("\"{s}\""));
            }
            Type::TypeRef(p, n) => {
                self.type_prefix(p)?;
                self.out.push_str(n.strip_suffix('$').unwrap_or(&n));
            }
            Type::Sym(p, s) if self.u.symbol(s).kind.is_type() => {
                match p {
                    Some(p) => self.type_prefix(p)?,
                    None => self.owner_prefix(s)?,
                }
                let name = self.u.symbol(s).name.clone();
                self.out.push_str(name.strip_suffix('$').unwrap_or(&name));
            }
            Type::Sym(p, s) => {
                let name = self.u.symbol(s).name.clone();
                match p {
                    Some(p) => self.term_ref(p, &name)?,
                    None => {
                        self.owner_prefix(s)?;
                        self.out.push_str(&name);
                    }
                }
            }
            Type::TermRef(p, n) => self.term_ref(p, &n)?,
            Type::This(c) => self.this(c)?,
            Type::Package(s) if s != self.u.root() => self.out.push_str(&self.u.full_name(s)),
            Type::Applied(tycon, args) => self.applied(tycon, &args)?,
            Type::And(a, b) => self.infix(a, " & ", b)?,
            Type::Or(a, b, _) => self.infix(a, " | ", b)?,
            Type::ByName(r) => {
                self.out.push_str(" => ");
                self.print(r)?;
            }
            Type::Bounds(lo, hi) => {
                self.out.push('_');
                self.bounds(lo, hi)?;
            }
            Type::Alias(a) => {
                self.out.push('_');
                self.bounds(a, a)?;
            }
            Type::Lambda { binder, params, result } => {
                self.binders.push((binder, params.iter().map(|(n, _)| n.clone()).collect()));
                self.out.push('[');
                for (i, (name, info)) in params.iter().enumerate() {
                    if i > 0 {
                        self.out.push_str(", ");
                    }
                    self.out.push_str(name);
                    match *self.u.types().get(*info) {
                        Type::Bounds(lo, hi) => self.bounds(lo, hi)?,
                        _ => return Err(REFUSED),
                    }
                }
                self.out.push_str("] =>> ");
                self.print(result)?;
                self.binders.pop();
            }
            Type::Param(b, i) => {
                let names = self.binders.iter().rev().find(|(binder, _)| *binder == b);
                let name = names.and_then(|(_, n)| n.get(i as usize)).ok_or(REFUSED)?;
                self.out.push_str(&name.clone());
            }
            Type::Rec(_, p) => self.print(p)?,
            Type::RecThis(_) => self.out.push_str("this"),
            _ => return Err(REFUSED),
        }
        Ok(())
    }

    fn infix(&mut self, a: TypeId, op: &str, b: TypeId) -> Shown {
        self.print(a)?;
        self.out.push_str(op);
        self.print(b)
    }

    fn bounds(&mut self, lo: TypeId, hi: TypeId) -> Shown {
        self.out.push_str(" >: ");
        self.print(lo)?;
        self.out.push_str(" <: ");
        self.print(hi)
    }

    fn applied(&mut self, tycon: TypeId, args: &[TypeId]) -> Shown {
        let class = self.u.class_symbol(tycon);
        if class.is_some_and(|c| self.u.full_name(c) == "scala.<repeated>") {
            self.out.push_str("_*");
            return Ok(());
        }
        if matches!(self.u.types().get(tycon), Type::Lambda { .. }) {
            self.out.push('(');
            self.print(tycon)?;
            self.out.push(')');
        } else {
            self.print(tycon)?;
        }
        self.out.push('[');
        for (i, &a) in args.iter().enumerate() {
            if i > 0 {
                self.out.push_str(", ");
            }
            self.print(a)?;
        }
        self.out.push(']');
        Ok(())
    }

    /// A term reference `name` through prefix `p`.
    fn term_ref(&mut self, p: TypeId, name: &str) -> Shown {
        if !self.is_elided(p) && !matches!(self.u.types().get(p), Type::NoPrefix) {
            self.print(p)?;
            if name == "package" {
                return Ok(());
            }
            self.out.push('.');
        }
        self.out.push_str(name);
        Ok(())
    }

    /// The prefix of a type reference, `.` or `#` included.
    fn type_prefix(&mut self, p: TypeId) -> Shown {
        if self.is_elided(p) {
            return Ok(());
        }
        match self.u.types().get(p).clone() {
            Type::NoPrefix => Err(REFUSED),
            Type::Sym(_, s) | Type::Package(s) if self.u.symbol(s).kind == Kind::Package => {
                self.package(s);
                Ok(())
            }
            Type::Sym(_, s) if self.u.symbol(s).kind == Kind::Class => {
                self.print(p)?;
                self.out.push('#');
                Ok(())
            }
            _ => {
                self.print(p)?;
                self.out.push('.');
                Ok(())
            }
        }
    }

    /// The prefix dotty gives a reference to `s` without one: its package, its object or its
    /// class's `this`; none for a local or a type parameter.
    fn owner_prefix(&mut self, s: SymbolId) -> Shown {
        let Some(owner) = self.u.symbol(s).owner else { return Ok(()) };
        if self.u.symbol(s).kind == Kind::TypeParam || self.u.symbol(owner).kind.is_term() {
            return Ok(());
        }
        match self.u.symbol(owner).kind {
            Kind::Package => {
                self.package(owner);
                Ok(())
            }
            Kind::Class if self.u.is_module_class(owner) => {
                let module = self.u.source_module(owner).ok_or(REFUSED)?;
                self.owner_prefix(module)?;
                self.out.push_str(&self.u.symbol(module).name.clone());
                self.out.push('.');
                Ok(())
            }
            _ => Err(REFUSED),
        }
    }

    /// Package `s` as a prefix: its full name and `.`, nothing for the root.
    fn package(&mut self, s: SymbolId) {
        if s != self.u.root() {
            let name = self.u.full_name(s);
            self.out.push_str(&name);
            self.out.push('.');
        }
    }

    /// `C.this` (dotty's `ThisType`): a package by its name, an object by its path.
    fn this(&mut self, c: TypeId) -> Shown {
        let class = match *self.u.types().get(c) {
            Type::Package(p) => p,
            _ => self.u.class_symbol(c).ok_or(REFUSED)?,
        };
        match self.u.symbol(class).kind {
            Kind::Package if class == self.u.root() => Err(REFUSED),
            Kind::Package => {
                self.out.push_str(&self.u.full_name(class));
                Ok(())
            }
            _ if self.u.is_module_class(class) => {
                let module = self.u.source_module(class).ok_or(REFUSED)?;
                self.owner_prefix(module)?;
                self.out.push_str(&self.u.symbol(module).name.clone());
                Ok(())
            }
            _ => {
                let name = self.u.symbol(class).name.clone();
                self.out.push_str(&format!("{name}.this"));
                Ok(())
            }
        }
    }

    /// Whether prefix `p` is the root's or the empty package's `this`, which dotty omits.
    fn is_elided(&mut self, p: TypeId) -> bool {
        let root = self.u.root();
        match *self.u.types().get(p) {
            Type::This(c) => match *self.u.types().get(c) {
                Type::Package(s) => s == root,
                _ => self.u.class_symbol(c) == Some(root),
            },
            Type::Package(s) => s == root,
            _ => false,
        }
    }
}
