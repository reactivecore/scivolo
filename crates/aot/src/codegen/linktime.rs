//! `linktime if` conditions, decided while compiling, with the VM's property values except
//! multithreading, which executables run without.

use crate::R;
use nir::{Bin, Comp, Cond, Pool, Val};
use vm::Prop;

const MULTITHREADING: &str = "scala.scalanative.meta.linktimeinfo.isMultithreadingEnabled";

/// The value of the `LinktimeInfo` accessor `M<len><owner>D<len><method>…` names: the target
/// platform for the `is<OS>` ones, else the property it stands for.
fn accessor(mangled: &str) -> Option<Prop> {
    let (owner, sig) = ident(mangled.strip_prefix('M')?)?;
    let (method, _) = ident(sig.strip_prefix('D')?)?;
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
        _ => vm::property(&vm::accessor_property(owner, method)?),
    }
}

/// Splits `<len>[-]<name>…` after its name, `len` ASCII characters long.
fn ident(s: &str) -> Option<(&str, &str)> {
    let digits = s.find(|c: char| !c.is_ascii_digit())?;
    let len: usize = s[..digits].parse().ok()?;
    let body = s[digits..].strip_prefix('-').unwrap_or(&s[digits..]);
    body.is_char_boundary(len).then(|| body.split_at(len))
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
            let actual = match name {
                MULTITHREADING => Prop::Bool(false),
                _ => match vm::property(name).or_else(|| accessor(name)) {
                    Some(p) => p,
                    None => return Err(format!("link-time property {name}")),
                },
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
