//! `linktime if` conditions, decided while compiling, with the VM's property values except
//! multithreading, which executables run without (ADR 0028).

use super::body::FnBuilder;
use crate::R;
use cranelift_codegen::ir::{InstBuilder, Value, types};
use cranelift_module::Module;
use nir::{Attr, Bin, Comp, Cond, GlobalId, Pool, Val};
use vm::Prop;

impl FnBuilder<'_, '_> {
    pub fn is_linktime_resolved(&mut self, g: GlobalId) -> bool {
        self.reach.world.defn(g).is_ok_and(|d| d.attrs.iter().any(|a| matches!(a, Attr::LinktimeResolved)))
    }

    /// The value a call of `@resolvedAtLinktime` method `g` returns.
    pub fn linktime_value(&mut self, g: GlobalId) -> R<Value> {
        Ok(match resolved(&self.reach.world.pool, g)? {
            Prop::Bool(b) => self.b.ins().iconst(types::I8, b as i64),
            Prop::Int(i) => self.b.ins().iconst(types::I32, i as u32 as i64),
            Prop::Str(s) => {
                let units: Vec<u16> = s.encode_utf16().collect();
                let data = self.decls.string(self.module, self.reach, &units)?;
                let gv = self.module.declare_data_in_func(data, self.b.func);
                self.b.ins().symbol_value(self.decls.ptr, gv)
            }
        })
    }
}

const MULTITHREADING: &str = "scala.scalanative.meta.linktimeinfo.isMultithreadingEnabled";

/// The value of link-time property `name` in an executable.
fn property_value(name: &str) -> Option<Prop> {
    match name {
        MULTITHREADING => Some(Prop::Bool(false)),
        _ => vm::property(name),
    }
}

/// The value of the `LinktimeInfo` accessor `M<len><owner>D<len><method>…` names: the target
/// platform for the `is<OS>` ones, else the property it stands for.
fn accessor(mangled: &str) -> Option<Prop> {
    let (owner, sig) = vm::parse_member(mangled)?;
    let method = vm::method_name(sig)?;
    let os = |os: &str| Some(Prop::Bool(std::env::consts::OS == os));
    match method {
        "isWindows" => os("windows"),
        "isLinux" => os("linux"),
        "isMac" => os("macos"),
        "isFreeBSD" => os("freebsd"),
        "isOpenBSD" => os("openbsd"),
        "isNetBSD" => os("netbsd"),
        "isImmix" => Some(Prop::Bool(true)),
        "isBoehm" | "isCommix" | "isNone" | "isExperimental" => Some(Prop::Bool(false)),
        "asanEnabled" | "isFreeBSD32" | "hasDebugInfo" => Some(Prop::Bool(false)),
        _ => property_value(&vm::accessor_property(owner, method)?),
    }
}

/// The value of `@resolvedAtLinktime` method `g`.
pub fn resolved(pool: &Pool, g: nir::GlobalId) -> R<Prop> {
    let mangled = pool.mangle(g);
    accessor(&mangled).ok_or_else(|| format!("link-time method {mangled}"))
}

pub fn decide(pool: &Pool, cond: &Cond) -> R<bool> {
    match cond {
        Cond::Complex { bin, left, right, .. } => {
            let l = decide(pool, left)?;
            match (bin, l) {
                (Bin::And, false) => Ok(false),
                (Bin::Or, true) => Ok(true),
                _ => decide(pool, right),
            }
        }
        Cond::Simple { property, comp, value, .. } => {
            let name = pool.str(*property);
            let Some(actual) = property_value(name).or_else(|| accessor(name)) else {
                return Err(format!("link-time property {name}"));
            };
            let equal = match (actual, pool.val(*value)) {
                (Prop::Bool(b), Val::True) => b,
                (Prop::Bool(b), Val::False) => !b,
                (Prop::Int(i), Val::Int(j)) => i == *j,
                (Prop::Str(s), Val::String(t)) => s == pool.str(*t),
                (_, v) => return Err(format!("link-time value {v:?} for {name}")),
            };
            match comp {
                Comp::Ieq => Ok(equal),
                Comp::Ine => Ok(!equal),
                c => Err(format!("link-time comparison {c:?}")),
            }
        }
    }
}
