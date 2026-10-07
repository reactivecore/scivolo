//! Types and definitions as dotty's REPL shows them to users (`ReplPrinter` over
//! `RefinedPrinter` and `PlainPrinter`): `List[Int]`, `Int => String`, `def f[A](x: A): A`.

mod applied;
mod constant;
mod def;
mod prefix;

use super::{Binder, Type, TypeId};
use crate::tags::OBJECT;
use crate::{Kind, SymbolId, Universe};
use rustc_hash::FxHashSet;

/// Precedences of dotty's printer: `GlobalPrec`, `InfixPrec` and `DotPrec`.
const GLOBAL: u8 = 0;
const INFIX: u8 = 1;
const DOT: u8 = 11;

impl Universe {
    /// Type `t` as the REPL shows it.
    pub fn show_user(&mut self, t: TypeId) -> String {
        Printer::new(self).text(t, GLOBAL)
    }

    /// The definition of `s` as the REPL shows it: `def f(x: Int): Int`, `val x: Int`,
    /// `type T = Int`, `class C`.
    pub fn show_def(&mut self, s: SymbolId) -> String {
        Printer::new(self).def(s)
    }

    /// Static module class `s` as the REPL shows a reference to it through its owner's `this`, with the
    /// qualified name of that owner after any REPL wrapper object: `scala.util.Try`, `None`.
    pub fn show_module_class(&mut self, s: SymbolId) -> String {
        let full = Printer::new(self).owner_path(s).full;
        let owner = match full.rfind("rs$line$") {
            Some(at) => full[at..].split_once('.').map_or("", |(_, rest)| rest).to_owned(),
            None => full,
        };
        let name = self.symbol(s).name.clone();
        let prefix = prefix::prefix_text(prefix::Path { shown: owner.clone(), full: owner });
        format!("{prefix}{}", name.strip_suffix('$').unwrap_or(&name))
    }
}

pub struct Printer<'u> {
    pub u: &'u mut Universe,
    /// The classes shown without prefix: those aliased in `Predef` or the `scala` package object.
    pub bare: FxHashSet<SymbolId>,
    /// The parameter names of the enclosing binding types.
    pub binders: Vec<(Binder, Vec<String>)>,
}

/// `text` in parentheses if its precedence `own` is below the context's `prec`.
pub fn paren(own: u8, prec: u8, text: String) -> String {
    match own < prec {
        true => format!("({text})"),
        false => text,
    }
}

/// `name`, `_` for a wildcard parameter's synthetic name.
pub fn simple(name: &str) -> &str {
    if name.starts_with("_$") { "_" } else { name }
}

impl<'u> Printer<'u> {
    fn new(u: &'u mut Universe) -> Self {
        let bare = prefix::bare_classes(u);
        Printer { u, bare, binders: Vec::new() }
    }

    /// Type `t` in a context of precedence `prec`.
    pub fn text(&mut self, t: TypeId, prec: u8) -> String {
        match self.u.types().get(t).clone() {
            Type::NoPrefix => String::new(),
            Type::TypeRef(p, n) => {
                let s = self.u.resolve(p, &n, true);
                self.named(Some(p), s, &n)
            }
            Type::Sym(_, s) if self.u.symbol(s).kind.is_term() => format!("{}.type", self.path(t).shown),
            Type::Sym(p, s) => {
                let name = self.u.symbol(s).name.clone();
                self.named(p, Some(s), &name)
            }
            Type::TermRef(..) | Type::This(_) | Type::Package(_) | Type::PackageName(_) => {
                format!("{}.type", self.path(t).shown)
            }
            Type::Local(_, n) => simple(&n).to_owned(),
            Type::Applied(c, args) => self.applied(c, &args, prec),
            Type::Alias(_) | Type::Bounds(..) => self.rhs(t),
            Type::And(a, b) => self.infix("&", a, b, prec),
            Type::Or(a, b) => self.infix("|", a, b, prec),
            Type::Annotated(inner, annot) => self.annotated(inner, annot, prec),
            Type::ByName(r) => paren(GLOBAL, prec, format!("=> {}", self.text(r, GLOBAL))),
            Type::Refined(..) => self.refined(t, prec),
            Type::Rec(_, p) => self.text(p, prec),
            Type::RecThis(_) => "this".into(),
            Type::Lambda { .. } => match self.eta_contracted(t) {
                Some(tycon) => self.text(tycon, prec),
                None => paren(GLOBAL, prec, self.lambda(t)),
            },
            Type::Method { .. } | Type::Poly { .. } => paren(GLOBAL, prec, self.lambda(t)),
            Type::Param(b, i) => self.param(b, i),
            Type::Match(bound) => self.text(bound, prec),
            Type::Constant(crate::tags::CLASSconst, bits) => {
                format!("classOf[{}]", self.text(TypeId(bits as u32), GLOBAL))
            }
            Type::Constant(tag, bits) => constant::text(tag, bits),
            Type::Str(s) => format!("\"{s}\""),
            Type::Other(tag) => format!("<{}>", crate::tags::name(tag)),
            Type::Var(_) => "?".into(),
            Type::Wildcard(None) => "?".into(),
            Type::Wildcard(Some(b)) => format!("?{}", self.rhs(b)),
            Type::Error => "<error>".into(),
        }
    }

    /// A reference to type `name`, symbol `s` if resolved, through `prefix` or else its owners.
    fn named(&mut self, prefix: Option<TypeId>, s: Option<SymbolId>, name: &str) -> String {
        if let Some(s) = s {
            let sym = self.u.symbol(s);
            if sym.kind == Kind::TypeParam || self.bare.contains(&s) {
                return simple(&sym.name).to_owned();
            }
            if sym.kind == Kind::Class && sym.flags.has(OBJECT) {
                return format!("{}.type", self.symbol_path(prefix, s).shown);
            }
        }
        let prefix = match (prefix, s) {
            (Some(p), _) => self.prefix(p),
            (None, Some(s)) => {
                let path = self.owner_path(s);
                prefix::prefix_text(path)
            }
            (None, None) => String::new(),
        };
        format!("{prefix}{}", simple(name))
    }

    /// Parameter `i` of binding type `b`, by name.
    fn param(&self, b: Binder, i: u32) -> String {
        let names = self.binders.iter().rev().find(|(binder, _)| *binder == b);
        match names.and_then(|(_, names)| names.get(i as usize)) {
            Some(n) => simple(n).to_owned(),
            None => format!("<param {i}>"),
        }
    }

    /// `inner` annotated with `annot`: Java varargs `T*`; other annotations are dropped.
    fn annotated(&mut self, inner: TypeId, annot: TypeId, prec: u8) -> String {
        let repeated = self.full_name_of(annot).as_deref() == Some("scala.annotation.internal.Repeated");
        match self.u.types().get(inner).clone() {
            Type::Applied(_, args) if repeated && args.len() == 1 => format!("{}*", self.text(args[0], DOT)),
            _ => self.text(inner, prec),
        }
    }

    /// The qualified name of the class or type `t` refers to.
    pub fn full_name_of(&mut self, t: TypeId) -> Option<String> {
        match self.u.types().get(t).clone() {
            Type::Sym(p, s) => Some(self.symbol_path(p, s).full),
            Type::TypeRef(p, n) => Some(prefix::join(&self.path(p).full, &n)),
            _ => None,
        }
    }
}
