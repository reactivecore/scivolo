//! Link-time properties (`LinktimeInfo`) and the `linktime if` conditions built on them.

use crate::interp::Flow;
use crate::{Vm, VmError};
use nir::{Bin, Comp, Cond, StrId, Val};

const PREFIX: &str = "scala.scalanative.meta.linktimeinfo.";

enum Prop {
    Bool(bool),
    Int(i32),
    Str(&'static str),
}

fn property(name: &str) -> Option<Prop> {
    let key = name.strip_prefix(PREFIX)?;
    Some(match key {
        "debugMode"
        | "is32BitPlatform"
        | "isWeakReferenceSupported"
        | "isMultithreadingEnabled"
        | "isMsys"
        | "debugMetadata.enabled"
        | "debugMetadata.generateFunctionSourcePositions" => Prop::Bool(false),
        "releaseMode" => Prop::Bool(true),
        "contendedPaddingWidth" => Prop::Int(64),
        "runtimeVersion" => Prop::Str("0.5.12"),
        "garbageCollector" => Prop::Str("none"),
        "enabledSanitizer" | "target.env" => Prop::Str(""),
        "target.arch" => Prop::Str(if cfg!(target_arch = "aarch64") { "aarch64" } else { "x86_64" }),
        "target.vendor" => Prop::Str(if cfg!(target_os = "macos") { "apple" } else { "unknown" }),
        "target.os" => Prop::Str(if cfg!(target_os = "macos") { "darwin" } else { "linux" }),
        _ => return None,
    })
}

/// The property a `@resolvedAtLinktime` accessor of `owner` named `method` stands for.
pub fn accessor_property(owner: &str, method: &str) -> Option<String> {
    let scope = match owner {
        "scala.scalanative.meta.LinktimeInfo$" => "",
        "scala.scalanative.meta.LinktimeInfo$target$" => "target.",
        "scala.scalanative.meta.LinktimeInfo$sourceLevelDebuging$" => "debugMetadata.",
        _ => return None,
    };
    let name = format!("{PREFIX}{scope}{method}");
    property(&name).map(|_| name)
}

impl Vm {
    /// The value of property `name` in the VM's representation.
    pub fn property_value(&mut self, name: &str) -> Flow {
        match property(name) {
            Some(Prop::Bool(b)) => Ok(b as u64),
            Some(Prop::Int(i)) => Ok(i as u32 as u64),
            Some(Prop::Str(s)) => Ok(self.new_string(s)? as u64),
            None => Err(VmError::Unsupported(format!("link-time property {name}")).into()),
        }
    }

    pub fn linktime_cond(&mut self, cond: &Cond) -> Flow<bool> {
        match cond {
            Cond::Complex { bin, left, right, .. } => {
                let l = self.linktime_cond(left)?;
                match (bin, l) {
                    (Bin::And, false) => Ok(false),
                    (Bin::Or, true) => Ok(true),
                    _ => self.linktime_cond(right),
                }
            }
            Cond::Simple { property, comp, value, .. } => {
                let actual = self.property_or_accessor(*property)?;
                let expected = self.world.pool.val(*value).clone();
                let equal = match expected {
                    Val::String(s) => {
                        let s = self.world.pool.str(s).to_owned();
                        self.read_string(actual as usize)? == s
                    }
                    Val::True => actual & 1 == 1,
                    Val::False => actual & 1 == 0,
                    Val::Int(i) => actual as u32 == i as u32,
                    other => return Err(VmError::Unsupported(format!("link-time value {other:?}")).into()),
                };
                match comp {
                    Comp::Ieq => Ok(equal),
                    Comp::Ine => Ok(!equal),
                    other => Err(VmError::Unsupported(format!("link-time comparison {other:?}")).into()),
                }
            }
        }
    }

    /// A property by name, or the result of calling the mangled accessor method it names.
    fn property_or_accessor(&mut self, name: StrId) -> Flow {
        let text = self.world.pool.str(name).to_owned();
        if property(&text).is_some() {
            return self.property_value(&text);
        }
        let Some((top, sig)) = parse_member(&text) else {
            return Err(VmError::Unsupported(format!("link-time property {text}")).into());
        };
        let owner = self.world.top(&top);
        let g = self.world.member(owner, &sig);
        let module = self.module(owner)?;
        self.call_global(g, &[module as u64])
    }
}

/// The name of the method a mangled signature `D<len><name>...` describes.
pub fn method_name(sig: &str) -> Option<&str> {
    let rest = sig.strip_prefix('D')?;
    let digits = rest.chars().take_while(char::is_ascii_digit).count();
    let len: usize = rest[..digits].parse().ok()?;
    rest.get(digits..digits + len)
}

/// Splits a mangled member name `M<len><top><sig>` into its owner and signature.
fn parse_member(mangled: &str) -> Option<(String, String)> {
    let rest = mangled.strip_prefix('M')?;
    let digits = rest.chars().take_while(char::is_ascii_digit).count();
    let len: usize = rest[..digits].parse().ok()?;
    let rest = &rest[digits..];
    let top: String = rest.chars().take(len).collect();
    Some((top.clone(), rest[top.len()..].to_string()))
}
