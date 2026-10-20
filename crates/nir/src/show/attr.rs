// Derived from Scala Native 0.5.12 (Apache-2.0, see native/LICENSE-scala-native.md); translated to Rust and changed:
//   nir/src/main/scala/scala/scalanative/nir/Show.scala
use super::Printer;
use crate::nir::Attr;
use std::fmt::Write;

impl Printer<'_> {
    pub fn attrs(&mut self, attrs: &[Attr]) {
        let pool = self.pool;
        for a in attrs {
            let _ = match a {
                Attr::BailOpt(m) => write!(self.out, "bailopt(\"{}\")", pool.str(*m)),
                Attr::Extern { blocking } => {
                    write!(self.out, "extern{}", if *blocking { " blocking" } else { "" })
                }
                Attr::Link(n) => write!(self.out, "link(\"{}\")", pool.str(*n)),
                Attr::Define(n) => write!(self.out, "define(\"{}\")", pool.str(*n)),
                Attr::Align { size, group } => match group {
                    Some(g) => write!(self.out, "align({size}, {})", pool.str(*g)),
                    None => write!(self.out, "align({size})"),
                },
                other => write!(self.out, "{}", name(other)),
            };
            self.s(" ");
        }
    }
}

fn name(a: &Attr) -> &'static str {
    match a {
        Attr::MayInline => "mayinline",
        Attr::InlineHint => "inlinehint",
        Attr::NoInline => "noinline",
        Attr::AlwaysInline => "alwaysinline",
        Attr::MaySpecialize => "mayspecialize",
        Attr::NoSpecialize => "nospecialize",
        Attr::UnOpt => "unopt",
        Attr::NoOpt => "noopt",
        Attr::DidOpt => "didopt",
        Attr::Dyn => "dyn",
        Attr::Stub => "stub",
        Attr::LinkCppRuntime => "linkCppRuntime",
        Attr::Abstract => "abstract",
        Attr::Volatile => "volatile",
        Attr::Final => "final",
        Attr::SafePublish => "safe-publish",
        Attr::LinktimeResolved => "linktime",
        Attr::UsesIntrinsic => "uses-intrinsics",
        _ => "",
    }
}
