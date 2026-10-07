//! The definitions of an input as dotty's `Rendering` prints them: `val x: Int = 1`,
//! `def f(x: Int): Int`, `// defined class C`.

use crate::show_methods::SUFFIX;
use crate::value::{self, Quote};
use tasty::tags::{ARTIFACT, LAZY, OBJECT, PRIVATE, SYNTHETIC};
use tasty::{Kind, SymbolId, Universe};
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
        Kind::Class => text(format!("// defined {}", u.show_def(d))),
        Kind::Type if u.alias_of(d).is_some() => text(format!("// defined alias {}", u.show_def(d))),
        Kind::Type => text(format!("// defined {}", u.show_def(d))),
        Kind::Val if flags.has(OBJECT) => None,
        Kind::Val | Kind::Param if flags.has(LAZY) => text(u.show_def(d)),
        Kind::Val | Kind::Param => {
            let info = u.info(d);
            let quote = value::quote(&u.erase(info));
            let show = Some((tasty::encode(&format!("{name}{SUFFIX}")), quote));
            let text = match name.strip_suffix("$assign") {
                Some(var) => format!("{var}: {}", u.show_user(info)),
                None => u.show_def(d),
            };
            Some(Definition { text, show })
        }
        Kind::Def => text(u.show_def(d)),
        Kind::Package | Kind::TypeParam => None,
    }
}

/// The line that prints definition `d` of module `module`, running its show method.
pub fn line(vm: &mut Vm, module: &str, d: &Definition) -> Result<String, VmError> {
    let Some((method, quote)) = &d.show else { return Ok(d.text.clone()) };
    let shown = vm.call_string(module, method)?;
    Ok(format!("{} = {}", d.text, value::render(&shown, *quote)))
}
