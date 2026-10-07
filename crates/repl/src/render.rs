//! The definitions of an input as dotty's `Rendering` prints them: `val x: Int = 1`,
//! `def f(x: Int): Int`, `// defined class C`.

use crate::show_methods::SUFFIX;
use crate::value::{self, Quote};
use tasty::tags::{ARTIFACT, CASE, LAZY, MUTABLE, OBJECT, PRIVATE, SYNTHETIC, TRAIT};
use tasty::{Kind, SymbolId, Type, TypeId, Universe};
use vm::{Vm, VmError};

/// A definition to print: its text, and for a value the show method and quoting of its value.
pub struct Definition {
    text: String,
    show: Option<(String, Quote)>,
}

/// The definitions of module class `module`, in source order.
pub fn definitions(u: &mut Universe, module: &str) -> Vec<Definition> {
    let Some(class) = module_class(u, module) else { return Vec::new() };
    let decls = u.symbol(class).decls.clone();
    decls.into_iter().filter_map(|d| definition(u, d)).collect()
}

/// The class of top-level module class `module`.
pub fn module_class(u: &mut Universe, module: &str) -> Option<SymbolId> {
    let root = u.root();
    u.decl(root, module).unwrap_or_default().into_iter().find(|&c| u.symbol(c).kind == Kind::Class)
}

fn definition(u: &mut Universe, d: SymbolId) -> Option<Definition> {
    let s = u.symbol(d);
    let (name, kind, flags) = (s.name.clone(), s.kind, s.flags);
    let hidden = [PRIVATE, SYNTHETIC, ARTIFACT].iter().any(|&f| flags.has(f));
    if hidden || name.ends_with(SUFFIX) || name.starts_with('<') || name.ends_with("_=") {
        return None;
    }
    let text = |text: String| Some(Definition { text, show: None });
    match kind {
        Kind::Class if flags.has(OBJECT) => text(format!("// defined object {}", name.trim_end_matches('$'))),
        Kind::Class if flags.has(TRAIT) => text(format!("// defined trait {name}")),
        Kind::Class if flags.has(CASE) => text(format!("// defined case class {name}")),
        Kind::Class => text(format!("// defined class {name}")),
        Kind::Type => {
            let info = u.info(d);
            let alias = match u.types().get(info) {
                Type::Alias(a) | Type::Bounds(a, _) => *a,
                _ => info,
            };
            text(format!("// defined alias type {name} = {}", show_type(u, alias)))
        }
        Kind::Val if flags.has(OBJECT) => None,
        Kind::Val | Kind::Param => {
            let info = u.info(d);
            let tpe = show_type(u, info);
            if flags.has(LAZY) {
                return text(format!("lazy val {name}: {tpe}"));
            }
            let quote = value::quote(&tpe);
            let show = Some((format!("{name}{SUFFIX}"), quote));
            let text = match name.strip_suffix("$assign") {
                Some(var) => format!("{var}: {tpe}"),
                None if flags.has(MUTABLE) => format!("var {name}: {tpe}"),
                None => format!("val {name}: {tpe}"),
            };
            Some(Definition { text, show })
        }
        Kind::Def => {
            let info = u.info(d);
            text(format!("def {name}{}", signature(u, info)))
        }
        Kind::Package | Kind::TypeParam => None,
    }
}

/// The parameter clauses and result type of a method of type `t`: `[T](x: T): T`.
fn signature(u: &Universe, t: TypeId) -> String {
    match u.types().get(t) {
        Type::Poly { params, result, .. } => {
            let names: Vec<&str> = params.iter().map(|(n, _)| n.as_str()).collect();
            let mut rest = signature(u, *result);
            for (i, n) in names.iter().enumerate() {
                rest = rest.replace(&format!("<param {i}>"), n);
            }
            format!("[{}]{rest}", names.join(", "))
        }
        Type::Method { params, result, contextual, implicit, .. } => {
            let using = match (*contextual, *implicit) {
                (true, _) => "using ",
                (false, true) => "implicit ",
                _ => "",
            };
            let params: Vec<String> =
                params.iter().map(|(n, t)| format!("{n}: {}", show_type(u, *t))).collect();
            format!("({using}{}){}", params.join(", "), signature(u, *result))
        }
        Type::ByName(r) => format!(": {}", show_type(u, *r)),
        _ => format!(": {}", show_type(u, t)),
    }
}

/// Type `t` as the REPL shows it to users.
pub fn show_type(u: &Universe, t: TypeId) -> String {
    let full = u.show(t);
    let mut out = String::with_capacity(full.len());
    let mut rest = full.as_str();
    'text: while let Some(c) = rest.chars().next() {
        if !out.ends_with(|p: char| p.is_alphanumeric() || p == '_' || p == '$' || p == '.') {
            for prefix in OMITTED_PREFIXES {
                if let Some(after) = rest.strip_prefix(prefix).filter(|a| a.starts_with(char::is_uppercase)) {
                    rest = after;
                    continue 'text;
                }
            }
            if let Some(after) = rest.strip_prefix("rs$line$") {
                let after = after.trim_start_matches(|c: char| c.is_ascii_digit());
                rest = after.strip_prefix('.').or(after.strip_prefix("$.")).unwrap_or(after);
                continue;
            }
        }
        out.push(c);
        rest = &rest[c.len_utf8()..];
    }
    out
}

/// The prefixes of the names in the root imports, which `show_type` omits.
const OMITTED_PREFIXES: [&str; 5] =
    ["scala.collection.immutable.", "scala.Predef.", "scala.package.", "java.lang.", "scala."];

/// The line that prints definition `d` of module `module`, running its show method.
pub fn line(vm: &mut Vm, module: &str, d: &Definition) -> Result<String, VmError> {
    let Some((method, quote)) = &d.show else { return Ok(d.text.clone()) };
    let shown = vm.call_string(module, method)?;
    Ok(format!("{} = {}", d.text, value::render(&shown, *quote)))
}
