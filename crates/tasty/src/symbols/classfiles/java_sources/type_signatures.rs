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
        let args = self.class_args(&c, parts);
        let (package, classes) = c.rsplit_once('/').map_or(("", c.as_str()), |(p, n)| (p, n));
        let mut sig = format!("L{package}{}", if package.is_empty() { "" } else { "/" });
        let mut applied = false;
        for (i, (name, args)) in classes.split('$').zip(args).enumerate() {
            if i > 0 {
                sig.push(if applied { '.' } else { '$' });
            }
            sig.push_str(name);
            if !args.is_empty() {
                applied = true;
                sig.push('<');
                for a in &args {
                    sig.push_str(&self.arg(u, a)?);
                }
                sig.push('>');
            }
        }
        Ok((format!("{sig};"), format!("L{c};")))
    }

    /// The type arguments of each class of `c$D$E` as name `parts` writes them, outermost first.
    /// An inner class named alone inside its generic outer class gets none for the outer class,
    /// unlike javac's signature: the reader then enters it as `Outer.this.Inner`, the type
    /// dotty gives it in a Java unit.
    fn class_args(&self, c: &str, parts: &[(String, Vec<Arg>)]) -> Vec<Vec<Arg>> {
        let count = c.rsplit('/').next().unwrap_or(c).split('$').count();
        let mut args = vec![Vec::new(); count];
        for (slot, (_, written)) in args.iter_mut().rev().zip(parts.iter().rev()) {
            slot.clone_from(written);
        }
        args
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
