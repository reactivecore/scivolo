//! Types as `scripts/typer-oracle` prints them.

use super::symbols::full_name;
use syntax::ast::constant::Constant;
use syntax::ast::print::quote;
use tasty::{Binder, Kind, SymbolId, Type, TypeId, Universe};

/// The deepest type nesting printed.
const MAX_DEPTH: u32 = 14;

/// Whether a reference with prefix `p` prints as its symbol's full name: no prefix, a package,
/// or the `this` of a package or object.
fn is_static_prefix(u: &mut Universe, p: Option<TypeId>) -> bool {
    let Some(p) = p else { return true };
    match u.types().get(p).clone() {
        Type::NoPrefix | Type::Package(_) | Type::PackageName(_) => true,
        Type::Sym(_, s) => u.symbol(s).kind == Kind::Package,
        Type::TermRef(q, name) => {
            u.resolve(q, &name, false).is_some_and(|s| u.symbol(s).kind == Kind::Package)
        }
        Type::This(c) => match u.types().get(c) {
            Type::Sym(_, c) => {
                let c = u.symbol(*c);
                c.kind == Kind::Package || c.flags.has(tasty::tags::OBJECT)
            }
            Type::Package(_) => true,
            _ => false,
        },
        _ => false,
    }
}

pub fn tpe(u: &mut Universe, t: TypeId) -> String {
    Printer { u, binders: Vec::new() }.tpe(t, 0)
}

/// A type printer and the parameter names of the binders it is inside, whether of term
/// parameters.
struct Printer<'a> {
    u: &'a mut Universe,
    binders: Vec<(Binder, Vec<String>, bool)>,
}

impl Printer<'_> {
    fn params(&mut self, ps: &[(String, TypeId)], n: u32) -> String {
        let shown: Vec<String> = ps
            .iter()
            .map(|(name, p)| {
                let bounds = matches!(self.u.types().get(*p), Type::Bounds(..));
                format!("{name}{}{}", if bounds { " " } else { ": " }, self.tpe(*p, n))
            })
            .collect();
        shown.join(", ")
    }

    /// `params` then `result` of the type with binder `binder`, of term parameters if `terms`,
    /// inside it.
    fn bound(
        &mut self,
        binder: Binder,
        ps: &[(String, TypeId)],
        result: TypeId,
        terms: bool,
        n: u32,
    ) -> (String, String) {
        self.binders.push((binder, ps.iter().map(|(name, _)| name.clone()).collect(), terms));
        let shown = (self.params(ps, n), self.tpe(result, n));
        self.binders.pop();
        shown
    }

    /// Whether `s` is a value parameter of a method type being printed, which TASTy refers to by
    /// its symbol and dotty by its parameter reference.
    fn is_bound_param(&mut self, s: SymbolId) -> bool {
        let sym = self.u.symbol(s);
        let (Kind::Param, Some(owner)) = (sym.kind, sym.owner) else { return false };
        let name = sym.name.clone();
        let binders = crate::signatures::method_binders(self.u, owner);
        self.binders.iter().any(|(b, names, terms)| *terms && binders.contains(b) && names.contains(&name))
    }

    fn tpe(&mut self, t: TypeId, d: u32) -> String {
        if d > MAX_DEPTH {
            return "…".into();
        }
        let n = d + 1;
        match self.u.types().get(t).clone() {
            Type::Sym(None, s) if self.is_bound_param(s) => self.u.symbol(s).name.clone(),
            Type::Sym(p, s) => {
                let k = if self.u.is_type_symbol(s) { "T:" } else { "t:" };
                if is_static_prefix(self.u, p) {
                    format!("{k}{}", full_name(self.u, s))
                } else {
                    let name = self.u.symbol(s).name.replace("<init>$default$", "$lessinit$greater$default$");
                    format!("{}.{k}{name}", self.tpe(p.expect("a prefix"), n))
                }
            }
            Type::TypeRef(p, name) | Type::TermRef(p, name) => {
                let ty = matches!(self.u.types().get(t), Type::TypeRef(..));
                match self.u.resolve(p, &name, ty) {
                    Some(s) => {
                        let sym = self.u.intern_type(Type::Sym(Some(p), s));
                        self.tpe(sym, d)
                    }
                    None => format!("{}.{}{name}", self.tpe(p, n), if ty { "T:" } else { "t:" }),
                }
            }
            Type::Local(_, name) => format!("T:{name}"),
            Type::Package(p) => format!("t:{}", full_name(self.u, p)),
            Type::PackageName(p) => format!("t:{p}"),
            Type::This(c) => format!("{}.this", self.tpe(c, n)),
            Type::Super(t, p) => format!("{}.super[{}]", self.tpe(t, n), self.tpe(p, n)),
            Type::Applied(c, args) => {
                let args: Vec<String> = args.iter().map(|&a| self.tpe(a, n)).collect();
                format!("{}[{}]", self.tpe(c, n), args.join(" "))
            }
            Type::Alias(a) => format!("= {}", self.tpe(a, n)),
            Type::Bounds(lo, hi) => format!(">: {} <: {}", self.tpe(lo, n), self.tpe(hi, n)),
            Type::And(a, b) => format!("({} & {})", self.tpe(a, n), self.tpe(b, n)),
            Type::Or(a, b) => format!("({} | {})", self.tpe(a, n), self.tpe(b, n)),
            Type::Annotated(a, annot) => match self.u.class_symbol(annot) {
                Some(c) => format!("{} @{}", self.tpe(a, n), full_name(self.u, c)),
                None => self.tpe(a, n),
            },
            Type::ByName(r) => format!("=> {}", self.tpe(r, n)),
            Type::Refined(p, name, info) => format!("{}{{{name} {}}}", self.tpe(p, n), self.tpe(info, n)),
            Type::Rec(_, p) => format!("rec{{{}}}", self.tpe(p, n)),
            Type::RecThis(_) => "rec.this".into(),
            Type::Method { binder, params: ps, result, implicit, contextual } => {
                let using = match (contextual, implicit) {
                    (true, _) => "using ",
                    (false, true) => "implicit ",
                    _ => "",
                };
                let (ps, result) = self.bound(binder, &ps, result, true, n);
                format!("{using}({ps}){result}")
            }
            Type::Poly { binder, params: ps, result } => {
                let (ps, result) = self.bound(binder, &ps, result, false, n);
                format!("[{ps}]{result}")
            }
            Type::Lambda { binder, params: ps, result } => {
                let (ps, result) = self.bound(binder, &ps, result, false, n);
                format!("[{ps}] =>> {result}")
            }
            Type::Param(b, i) => {
                let found = self.binders.iter().rev().find(|(binder, _, _)| *binder == b);
                let name = found.and_then(|(_, names, _)| names.get(i as usize)).map_or("", String::as_str);
                match found {
                    Some((_, _, true)) => name.to_owned(),
                    _ => format!("{name}#{i}"),
                }
            }
            Type::Match(bound) => format!("(<match> <: {})", self.tpe(bound, n)),
            Type::Constant(tasty::tags::CLASSconst, bits) => {
                format!("#Class:{}", self.tpe(TypeId(bits as u32), n))
            }
            Type::Constant(tag, bits) => constant(tag, bits),
            Type::Str(s) => format!("#String:{}", quote(&s.encode_utf16().collect::<Vec<_>>())),
            Type::Var(v) => format!("?tv{}", v.0),
            Type::Wildcard(None) => "?".into(),
            Type::Wildcard(Some(b)) => format!("?{}", self.tpe(b, n)),
            Type::Error => "<error>".into(),
            Type::NoPrefix => "<noprefix>".into(),
            Type::Other(tasty::tags::TEMPLATE) => "<class>".into(),
            Type::Other(tag) => format!("<{}>", tasty::tags::name(tag)),
        }
    }
}

/// A constant type as `#Tag:"value"`.
fn constant(tag: u8, bits: i64) -> String {
    use tasty::tags::*;
    let (name, value) = match tag {
        UNITconst => return "#Unit".into(),
        NULLconst => return "#Null".into(),
        TRUEconst => ("Boolean", "true".to_string()),
        FALSEconst => ("Boolean", "false".to_string()),
        BYTEconst => ("Byte", bits.to_string()),
        SHORTconst => ("Short", bits.to_string()),
        CHARconst => return format!("#Char:{}", quote(&[bits as u16])),
        INTconst => ("Int", bits.to_string()),
        LONGconst => ("Long", bits.to_string()),
        FLOATconst => ("Float", text(Constant::Float(f32::from_bits(bits as u32)))),
        DOUBLEconst => ("Double", text(Constant::Double(f64::from_bits(bits as u64)))),
        _ => return format!("#{}", tasty::tags::name(tag)),
    };
    format!("#{name}:{}", quote(&value.encode_utf16().collect::<Vec<_>>()))
}

fn text(c: Constant) -> String {
    String::from_utf16_lossy(&c.string_value())
}
