//! The generic signature and descriptor javac writes for a Java type.

use super::class_info::{OBJECT, Signatures};
use super::resolve::{Resolved, resolve};
use super::syntax::{Arg, Type};
use crate::symbols::Universe;

impl Signatures<'_> {
    /// Determines the generic signature and descriptor of type `t`; the refusal of a name not
    /// found or of type arguments of an outer class (`Outer<A>.Inner`).
    pub fn ty(&mut self, u: &mut Universe, t: &Type) -> Result<(String, String), String> {
        let parts = match t {
            Type::Primitive(c) => return Ok((c.to_string(), c.to_string())),
            Type::Array(e) => {
                let (sig, desc) = self.ty(u, e)?;
                return Ok((format!("[{sig}"), format!("[{desc}")));
            }
            Type::Named(parts) => parts,
        };
        let names: Vec<&str> = parts.iter().map(|(n, _)| n.as_str()).collect();
        let c = match resolve(u, self.sources, &self.scope, &names)? {
            Resolved::Var(v) => {
                let erased = self.erased_vars.iter().rev().find(|(n, _)| *n == v);
                let erased = erased.map_or_else(|| format!("L{OBJECT};"), |(_, e)| e.clone());
                return Ok((format!("T{v};"), erased));
            }
            Resolved::Class(c) => c,
        };
        if parts.iter().rev().skip(1).any(|(_, args)| !args.is_empty()) {
            return Err(format!("type arguments of an outer class in `{}`", names.join(".")));
        }
        let args = &parts.last().expect("a name").1;
        let mut sig = format!("L{c}");
        if !args.is_empty() {
            sig.push('<');
            for a in args {
                sig.push_str(&self.arg(u, a)?);
            }
            sig.push('>');
        }
        Ok((format!("{sig};"), format!("L{c};")))
    }

    fn arg(&mut self, u: &mut Universe, a: &Arg) -> Result<String, String> {
        Ok(match a {
            Arg::Wildcard => "*".to_owned(),
            Arg::Type(t) => self.ty(u, t)?.0,
            Arg::Extends(t) => format!("+{}", self.ty(u, t)?.0),
            Arg::Super(t) => format!("-{}", self.ty(u, t)?.0),
        })
    }
}
