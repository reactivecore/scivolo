//! Link-time properties (`LinktimeInfo`) and the `linktime if` conditions built on them.

use crate::interp::Flow;
use crate::{Vm, VmError};
use nir::{Bin, Comp, Cond, GlobalId, StrId, Val};
use rustc_hash::FxHashMap;

const PREFIX: &str = "scala.scalanative.meta.linktimeinfo.";
const MULTITHREADING: &str = "scala.scalanative.meta.linktimeinfo.isMultithreadingEnabled";

/// Evaluated properties and conditions.
#[derive(Default)]
pub struct Linktime {
    pub props: FxHashMap<String, u64>,
    conds: FxHashMap<usize, bool>,
}

pub enum Prop {
    Bool(bool),
    Int(i32),
    Str(&'static str),
}

/// The value of link-time property `name`, if it is one the VM knows.
pub fn property(name: &str) -> Option<Prop> {
    let key = name.strip_prefix(PREFIX)?;
    Some(match key {
        "debugMode"
        | "is32BitPlatform"
        | "isMsys"
        | "isCygwin"
        | "debugMetadata.enabled"
        | "debugMetadata.generateFunctionSourcePositions" => Prop::Bool(false),
        "releaseMode" | "isWeakReferenceSupported" | "isMultithreadingEnabled" => Prop::Bool(true),
        "contendedPaddingWidth" => Prop::Int(64),
        "runtimeVersion" => Prop::Str("0.5.12"),
        "garbageCollector" => Prop::Str("immix"),
        "enabledSanitizer" => Prop::Str(""),
        "target.env" => Prop::Str(if cfg!(target_os = "macos") { "unknown" } else { "gnu" }),
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
        if let Some(&v) = self.linktime.props.get(name) {
            return Ok(v);
        }
        let v = match property(name) {
            Some(Prop::Bool(b)) => b as u64,
            Some(Prop::Int(i)) => i as u32 as u64,
            Some(Prop::Str(s)) => self.new_string_in(s, crate::object::Space::Permanent)? as u64,
            None => return Err(VmError::Unsupported(format!("link-time property {name}")).into()),
        };
        self.linktime.props.insert(name.to_owned(), v);
        Ok(v)
    }

    /// Evaluates `cond` in method `method` once; definitions live as long as the VM, so the
    /// address is a key. `MemoryLayout` and `Object.clone` see multithreading disabled: objects
    /// have no lock word.
    pub fn linktime_cond(&mut self, cond: &Cond, method: GlobalId) -> Flow<bool> {
        let key = cond as *const Cond as usize;
        if let Some(&b) = self.linktime.conds.get(&key) {
            return Ok(b);
        }
        let owner = self.world.pool.top_name(method);
        let sig = self.world.pool.member_sig(method).unwrap_or("");
        let layout = owner.starts_with("scala.scalanative.runtime.MemoryLayout$")
            || (owner == "java.lang.Object" && sig.starts_with("D5clone"));
        let b = self.eval_cond(cond, layout)?;
        self.linktime.conds.insert(key, b);
        Ok(b)
    }

    fn eval_cond(&mut self, cond: &Cond, layout: bool) -> Flow<bool> {
        match cond {
            Cond::Complex { bin, left, right, .. } => {
                let l = self.eval_cond(left, layout)?;
                match (bin, l) {
                    (Bin::And, false) => Ok(false),
                    (Bin::Or, true) => Ok(true),
                    _ => self.eval_cond(right, layout),
                }
            }
            Cond::Simple { property, comp, value, .. } => {
                let multithreading = self.world.pool.str(*property) == MULTITHREADING;
                let actual = match layout && multithreading {
                    true => 0,
                    false => self.property_or_accessor(*property)?,
                };
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
        let owner = self.world.top(top);
        let g = self.world.member(owner, sig);
        let module = self.module(owner)?;
        self.call_global(g, &[module as u64])
    }
}

/// The name of the method a mangled signature `D<len><name>...` describes.
pub fn method_name(sig: &str) -> Option<&str> {
    ident(sig.strip_prefix('D')?).map(|(name, _)| name)
}

/// Splits a mangled member name `M<len><top><sig>` into its owner and signature.
pub fn parse_member(mangled: &str) -> Option<(&str, &str)> {
    ident(mangled.strip_prefix('M')?)
}

/// Splits `<len>[-]<chars>...` after its identifier, whose length counts UTF-16 units.
fn ident(s: &str) -> Option<(&str, &str)> {
    let digits = s.chars().take_while(char::is_ascii_digit).count();
    let len: usize = s[..digits].parse().ok()?;
    let body = &s[digits..];
    let body = body.strip_prefix('-').unwrap_or(body);
    let mut units = 0;
    let end = body.char_indices().find_map(|(i, c)| {
        let at = (units == len).then_some(i);
        units += c.len_utf16();
        at
    });
    let end = end.or_else(|| (units == len).then_some(body.len()))?;
    Some(body.split_at(end))
}
