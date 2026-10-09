use super::{member, package};
use tasty::{Kind, SymbolId, Universe};

/// `Object.synchronized` and the methods `scala.scalanative.runtime.enterMonitor` and
/// `exitMonitor` it runs its body between.
#[derive(Clone, Copy)]
pub struct Monitors {
    pub synchronized: SymbolId,
    pub enter: SymbolId,
    pub exit: SymbolId,
}

impl Monitors {
    pub fn new(u: &mut Universe, object: SymbolId) -> Result<Monitors, String> {
        let runtime = package(u, "scala.scalanative.runtime")?;
        let module = member(u, runtime, "package", Kind::Val)?;
        let class = u.module_class(module).ok_or("scala.scalanative.runtime.package's class missing")?;
        Ok(Monitors {
            synchronized: member(u, object, "synchronized", Kind::Def)?,
            enter: member(u, class, "enterMonitor", Kind::Def)?,
            exit: member(u, class, "exitMonitor", Kind::Def)?,
        })
    }
}
