// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/scala/quoted/runtime/impl/printers/SourceCode.scala
//! Types as `quotes.reflect`'s `Printer.TypeReprCode` shows them (dotty's `SourceCode.printType`
//! with full names): `scala.collection.immutable.List[scala.Int]`, `scala.Predef.String`; and
//! as `Printer.TypeReprShortCode` shows them, without prefixes.

mod paths;
mod refinements;

use super::{Binder, Type, TypeId};
use crate::Universe;
use crate::tags::INFIX;
use crate::view::{Const, constant};

type Shown = Result<(), &'static str>;

const REFUSED: &str = "TypeRepr.show of a type the frontend does not print";

impl Universe {
    /// Type `t` as `Printer.TypeReprCode` shows it, `Printer.TypeReprShortCode` unless `full`.
    pub fn show_source(&mut self, t: TypeId, full: bool) -> Result<String, &'static str> {
        let mut p = SourcePrinter { u: self, binders: Vec::new(), out: String::new(), indent: 0, full };
        p.print(t)?;
        Ok(p.out)
    }
}

struct SourcePrinter<'u> {
    u: &'u mut Universe,
    /// The parameter names of the enclosing binding types.
    binders: Vec<(Binder, Vec<String>)>,
    out: String,
    indent: usize,
    /// Whether references print with their prefixes.
    full: bool,
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
            Type::NoPrefix => self.out.push_str("<NoPrefix() does not have a source representation>"),
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
                        match name == "package" {
                            true if self.full && self.out.ends_with('.') => _ = self.out.pop(),
                            _ => self.out.push_str(&name),
                        }
                    }
                }
            }
            Type::TermRef(p, n) => self.term_ref(p, &n)?,
            Type::This(c) => self.this(c)?,
            Type::Package(s) if s == self.u.root() => self.out.push_str("_root_"),
            Type::Package(s) if self.full => self.out.push_str(&self.u.full_name(s)),
            Type::Package(s) => self.out.push_str(&self.u.symbol(s).name.clone()),
            Type::Super(this, _) => {
                self.print(this)?;
                self.out.push_str(".super");
            }
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
            Type::Lambda { binder, params, result } => self.bind(binder, &params, |p| {
                p.out.push('[');
                p.params(&params)?;
                p.out.push_str("] =>> ");
                p.print(result)
            })?,
            Type::Method { binder, params, result, .. } => self.bind(binder, &params, |p| {
                p.out.push('(');
                p.params(&params)?;
                p.out.push(')');
                p.print(result)
            })?,
            Type::Poly { binder, params, result } => self.bind(binder, &params, |p| {
                p.out.push('[');
                for (i, &(ref name, info)) in params.iter().enumerate() {
                    p.out.push_str(if i > 0 { ", " } else { "" });
                    p.out.push_str(&format!("{name} "));
                    p.print(info)?;
                }
                p.out.push(']');
                p.print(result)
            })?,
            Type::Param(b, i) => {
                let names = self.binders.iter().rev().find(|(binder, _)| *binder == b);
                let name = names.and_then(|(_, n)| n.get(i as usize)).ok_or(REFUSED)?;
                self.out.push_str(&name.clone());
            }
            Type::Refined(..) => self.refinement(t)?,
            Type::Match { scrutinee, cases, .. } => self.match_type(scrutinee, &cases)?,
            Type::MatchCase(pat, body) => {
                self.out.push_str("case ");
                self.infix(pat, " => ", body)?;
            }
            Type::Rec(_, p) => self.print(p)?,
            Type::RecThis(_) => self.out.push_str("this"),
            _ => return Err(REFUSED),
        }
        Ok(())
    }

    /// Runs `f` inside the binder `binder` of parameters `params`.
    fn bind(
        &mut self,
        binder: Binder,
        params: &[(String, TypeId)],
        f: impl FnOnce(&mut Self) -> Shown,
    ) -> Shown {
        self.binders.push((binder, params.iter().map(|(n, _)| n.clone()).collect()));
        let shown = f(self);
        self.binders.pop();
        shown
    }

    /// Parameters `params` as `printMethodicTypeParams` shows them.
    fn params(&mut self, params: &[(String, TypeId)]) -> Shown {
        for (i, &(ref name, info)) in params.iter().enumerate() {
            self.out.push_str(if i > 0 { ", " } else { "" });
            self.out.push_str(name);
            match *self.u.types().get(info) {
                Type::Bounds(lo, hi) => self.bounds(lo, hi)?,
                Type::Alias(a) => self.bounds(a, a)?,
                _ => {
                    self.out.push_str(": ");
                    self.print(info)?;
                }
            }
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
        self.infix(lo, " <: ", hi)
    }

    fn applied(&mut self, tycon: TypeId, args: &[TypeId]) -> Shown {
        let class = self.u.class_symbol(tycon);
        if class.is_some_and(|c| self.u.full_name(c) == "scala.<repeated>") {
            self.out.push_str("_*");
            return Ok(());
        }
        let infix =
            matches!(*self.u.types().get(tycon), Type::Sym(_, s) if self.u.symbol(s).flags.has(INFIX));
        if !self.full && infix && args.len() == 2 {
            self.out.push('(');
            self.print(args[0])?;
            self.out.push(' ');
            self.print(tycon)?;
            self.out.push(' ');
            self.print(args[1])?;
            self.out.push(')');
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
            self.out.push_str(if i > 0 { ", " } else { "" });
            self.print(a)?;
        }
        self.out.push(']');
        Ok(())
    }
}
