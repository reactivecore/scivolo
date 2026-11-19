// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Namer.scala
//! Export forwarders of one name that erase alike: the one overloading resolution prefers stays
//! (dotty's `avoidClashes`).

use super::exports::{Export, Forwarder};
use crate::typer::Typer;
use tasty::Kind;

impl Typer<'_> {
    /// Drops from `e`'s forwarders each one that erases like a later one of the same name and
    /// whose target overloading resolution prefers less, the kept one in the place of the first;
    /// refuses two that it cannot order (dotty's `avoidClashes`).
    pub fn avoid_clashes(&mut self, e: &mut Export) {
        let mut i = 0;
        while i < e.out.len() {
            if self.u.symbol(e.out[i].sym).kind == Kind::Def {
                let mut j = i + 1;
                while j < e.out.len() && self.same_name(e.out[i], e.out[j]) {
                    if !self.clashes(e.out[i], e.out[j]) {
                        j += 1;
                        continue;
                    }
                    let (a, b) = (e.out[i], e.out[j]);
                    let cmp = self.compare_members(e.path, (a.target, b.target), e.ctx);
                    let Some(cmp) = cmp.filter(|&c| c != 0) else {
                        self.unsupported_at(b.span, "export forwarders that erase alike");
                        return;
                    };
                    let (kept, dropped) = if cmp < 0 { (b, a) } else { (a, b) };
                    self.u.unenter(dropped.sym);
                    e.out[i] = kept;
                    e.out.remove(j);
                }
            }
            i += 1;
        }
    }

    fn same_name(&self, a: Forwarder, b: Forwarder) -> bool {
        let (a, b) = (self.u.symbol(a.sym), self.u.symbol(b.sym));
        a.name == b.name && a.kind.is_type() == b.kind.is_type()
    }

    /// Whether forwarders `a` and `b` have the same target name and erased signature.
    fn clashes(&mut self, a: Forwarder, b: Forwarder) -> bool {
        let (sa, sb) = (self.u.symbol(a.sym), self.u.symbol(b.sym));
        let target = |s: &tasty::Symbol| s.target.clone().unwrap_or_else(|| s.name.clone());
        target(sa) == target(sb) && self.u.erased_method(a.sym) == self.u.erased_method(b.sym)
    }
}
