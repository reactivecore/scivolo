//! Types as `scripts/typer-oracle` prints them.

use super::symbols::full_name;
use crate::symbols::is_type_symbol;
use syntax::ast::constant::Constant;
use syntax::ast::print::quote;
use tasty::{Kind, Type, TypeId, Universe};

/// The deepest type nesting printed.
const MAX_DEPTH: u32 = 14;

/// Whether a reference with prefix `p` prints as its symbol's full name.
fn is_static_prefix(u: &Universe, p: Option<TypeId>) -> bool {
    let Some(p) = p else { return true };
    match u.types().get(p) {
        Type::NoPrefix | Type::Package(_) | Type::PackageName(_) => true,
        Type::This(c) => match u.types().get(*c) {
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
    tpe_at(u, t, 0)
}

fn tpe_at(u: &mut Universe, t: TypeId, d: u32) -> String {
    if d > MAX_DEPTH {
        return "…".into();
    }
    let n = d + 1;
    let params = |u: &mut Universe, ps: &[(String, TypeId)]| {
        let shown: Vec<String> = ps
            .iter()
            .map(|(name, p)| {
                let bounds = matches!(u.types().get(*p), Type::Bounds(..));
                format!("{name}{}{}", if bounds { " " } else { ": " }, tpe_at(u, *p, n))
            })
            .collect();
        shown.join(", ")
    };
    match u.types().get(t).clone() {
        Type::Sym(p, s) => {
            let k = if is_type_symbol(u, s) { "T:" } else { "t:" };
            if is_static_prefix(u, p) {
                format!("{k}{}", full_name(u, s))
            } else {
                format!("{}.{k}{}", tpe_at(u, p.expect("a prefix"), n), u.symbol(s).name)
            }
        }
        Type::TypeRef(p, name) | Type::TermRef(p, name) => {
            let ty = matches!(u.types().get(t), Type::TypeRef(..));
            match u.resolve(p, &name, ty) {
                Some(s) => {
                    let sym = u.intern_type(Type::Sym(Some(p), s));
                    tpe_at(u, sym, d)
                }
                None => format!("{}.{}{name}", tpe_at(u, p, n), if ty { "T:" } else { "t:" }),
            }
        }
        Type::Local(_, name) => format!("T:{name}"),
        Type::Package(p) => format!("t:{}", full_name(u, p)),
        Type::PackageName(p) => format!("t:{p}"),
        Type::This(c) => format!("{}.this", tpe_at(u, c, n)),
        Type::Applied(c, args) => {
            let args: Vec<String> = args.iter().map(|&a| tpe_at(u, a, n)).collect();
            format!("{}[{}]", tpe_at(u, c, n), args.join(" "))
        }
        Type::Alias(a) => format!("= {}", tpe_at(u, a, n)),
        Type::Bounds(lo, hi) => format!(">: {} <: {}", tpe_at(u, lo, n), tpe_at(u, hi, n)),
        Type::And(a, b) => format!("({} & {})", tpe_at(u, a, n), tpe_at(u, b, n)),
        Type::Or(a, b) => format!("({} | {})", tpe_at(u, a, n), tpe_at(u, b, n)),
        Type::Annotated(a, _) => tpe_at(u, a, n),
        Type::ByName(r) => format!("=> {}", tpe_at(u, r, n)),
        Type::Refined(p, name, info) => format!("{}{{{name} {}}}", tpe_at(u, p, n), tpe_at(u, info, n)),
        Type::Rec(_, p) => format!("rec{{{}}}", tpe_at(u, p, n)),
        Type::RecThis(_) => "rec.this".into(),
        Type::Method { params: ps, result, implicit, .. } => {
            let using = if implicit { "using " } else { "" };
            format!("{using}({}){}", params(u, &ps), tpe_at(u, result, n))
        }
        Type::Poly { params: ps, result, .. } => format!("[{}]{}", params(u, &ps), tpe_at(u, result, n)),
        Type::Lambda { params: ps, result, .. } => {
            format!("[{}] =>> {}", params(u, &ps), tpe_at(u, result, n))
        }
        Type::Param(_, i) => format!("#{i}"),
        Type::Match(bound) => format!("(<match> <: {})", tpe_at(u, bound, n)),
        Type::Constant(tag, bits) => constant(tag, bits),
        Type::Str(s) => format!("#String:{}", quote(&s.encode_utf16().collect::<Vec<_>>())),
        Type::Var(v) => format!("?tv{}", v.0),
        Type::Wildcard(None) => "?".into(),
        Type::Wildcard(Some(b)) => format!("?{}", tpe_at(u, b, n)),
        Type::Error => "<error>".into(),
        Type::NoPrefix => "<noprefix>".into(),
        Type::Other(tasty::tags::TEMPLATE) => "<class>".into(),
        Type::Other(tag) => format!("<{}>", tasty::tags::name(tag)),
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
        CHARconst => ("Char", char::from_u32(bits as u32).map_or(String::new(), String::from)),
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
