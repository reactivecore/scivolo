// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/classfile/ClassfileParser.scala
//! Java's descriptors and generic signatures (JVMS §4.3, §4.7.9.1) as types, as dotty's
//! `sigToType` reads them.

use super::super::Universe;
use crate::types::{Type, TypeId};
use crate::{ANY, NOTHING};

/// A type variable in scope: its name, its type, and whether it is bounded by `Object` only.
pub type Var = (String, TypeId, bool);

/// The bound of a type parameter bounded by `Object` only.
const OBJECT_BOUND: &str = ":Ljava/lang/Object;";

pub struct Sig<'a> {
    text: &'a str,
    at: usize,
    /// Whether type variables out of scope read as `Any` (dotty's `skiptvs`).
    skip: bool,
}

impl<'a> Sig<'a> {
    pub fn new(text: &'a str) -> Self {
        Sig { text, at: 0, skip: false }
    }

    fn peek(&self) -> Option<u8> {
        self.text.as_bytes().get(self.at).copied()
    }

    fn next(&mut self) -> Result<u8, String> {
        let c = self.peek().ok_or_else(|| format!("signature `{}` ends early", self.text))?;
        self.at += 1;
        Ok(c)
    }

    fn expect(&mut self, c: u8) -> Result<(), String> {
        match self.next()? == c {
            true => Ok(()),
            false => Err(format!("signature `{}`: `{}` expected at {}", self.text, c as char, self.at - 1)),
        }
    }

    /// The text up to the next of `ends`.
    fn name(&mut self, ends: &[u8]) -> Result<&'a str, String> {
        let rest = &self.text[self.at..];
        let n = rest
            .bytes()
            .position(|b| ends.contains(&b))
            .ok_or_else(|| format!("signature `{}`", self.text))?;
        self.at += n;
        Ok(&rest[..n])
    }

    /// The type parameters the signature starts with: each one's name and bounds' text.
    pub fn type_params(&mut self, u: &mut Universe) -> Result<Vec<(String, &'a str)>, String> {
        let mut params = Vec::new();
        if self.peek() != Some(b'<') {
            return Ok(params);
        }
        self.at += 1;
        let skip = std::mem::replace(&mut self.skip, true);
        while self.peek() != Some(b'>') {
            let name = self.name(b":")?.to_owned();
            let start = self.at;
            while self.peek() == Some(b':') {
                self.at += 1;
                if self.peek() != Some(b':') {
                    self.ty(u, &[])?;
                }
            }
            params.push((name, &self.text[start..self.at]));
        }
        self.skip = skip;
        self.at += 1;
        Ok(params)
    }

    /// The type of a class signature or reference type signature.
    pub fn ty(&mut self, u: &mut Universe, vars: &[Var]) -> Result<TypeId, String> {
        match self.next()? {
            b'L' => self.class_type(u, vars),
            b'[' => {
                let elem = self.ty(u, vars)?;
                let object = u.types.intern(Type::Sym(None, u.defn().object));
                let plain_var = vars.iter().any(|&(_, t, plain)| t == elem && plain);
                let elem = if plain_var { u.types.intern(Type::And(object, elem)) } else { elem };
                let array = u.class_ref("scala.Array");
                Ok(u.types.intern(Type::Applied(array, vec![elem])))
            }
            b'T' => {
                let name = self.name(b";")?;
                self.at += 1;
                let var = vars.iter().rev().find(|(n, ..)| n == name).map(|&(_, t, _)| t);
                var.or(self.skip.then_some(ANY)).ok_or_else(|| format!("type variable {name} out of scope"))
            }
            tag => {
                primitive(u, tag).ok_or_else(|| format!("signature `{}`: tag `{}`", self.text, tag as char))
            }
        }
    }

    fn class_type(&mut self, u: &mut Universe, vars: &[Var]) -> Result<TypeId, String> {
        let name = self.name(b";<.")?;
        let class = class_ref(u, name);
        let t = match self.peek() {
            Some(b'<') => {
                self.at += 1;
                let mut args = Vec::new();
                while self.peek() != Some(b'>') {
                    args.push(self.arg(u, vars)?);
                }
                self.at += 1;
                u.types.intern(Type::Applied(class, args))
            }
            _ if name == "java/lang/Object" => u.types.intern(Type::Sym(None, u.defn().from_java_object)),
            _ if self.skip => class,
            _ => cooked(u, name),
        };
        if self.peek() == Some(b'.') {
            return Err(format!("signature `{}`: a class nested in a generic class", self.text));
        }
        self.expect(b';')?;
        Ok(t)
    }

    fn arg(&mut self, u: &mut Universe, vars: &[Var]) -> Result<TypeId, String> {
        let from_java_object = u.types.intern(Type::Sym(None, u.defn().from_java_object));
        let bounds = match self.peek() {
            Some(b'*') => {
                self.at += 1;
                Type::Bounds(NOTHING, from_java_object)
            }
            Some(b'+') => {
                self.at += 1;
                Type::Bounds(NOTHING, self.ty(u, vars)?)
            }
            Some(b'-') => {
                self.at += 1;
                let lo = self.ty(u, vars)?;
                Type::Bounds(if lo == ANY { NOTHING } else { lo }, from_java_object)
            }
            _ => return self.ty(u, vars),
        };
        Ok(u.types.intern(bounds))
    }

    /// The parameter and result types of a method signature, the last array parameter of a
    /// varargs method `Array[T] @Repeated`; exceptions are skipped.
    pub fn method(
        &mut self,
        u: &mut Universe,
        vars: &[Var],
        varargs: bool,
    ) -> Result<(Vec<TypeId>, TypeId), String> {
        self.expect(b'(')?;
        let mut params = Vec::new();
        while self.peek() != Some(b')') {
            params.push(self.ty(u, vars)?);
        }
        self.at += 1;
        let result = self.ty(u, vars)?;
        if let Some(last) = params.last_mut().filter(|_| varargs)
            && u.is_array(*last)
        {
            let repeated = u.class_ref("scala.annotation.internal.Repeated");
            *last = u.types.intern(Type::Annotated(*last, repeated));
        }
        Ok((params, result))
    }

    /// Whether the signature is read to its end.
    pub fn at_end(&self) -> bool {
        self.at == self.text.len()
    }
}

/// The upper bound that bounds text `bounds` of a type parameter states: the intersection of
/// its types, `Any` for none (dotty's `sig2typeBounds`).
pub fn bounds(u: &mut Universe, bounds: &str, vars: &[Var]) -> Result<TypeId, String> {
    let mut sig = Sig::new(bounds);
    let mut upper: Option<TypeId> = None;
    while sig.peek() == Some(b':') {
        sig.at += 1;
        if sig.peek().is_some_and(|c| c != b':') {
            let t = sig.ty(u, vars)?;
            upper = Some(upper.map_or(t, |a| u.types.intern(Type::And(a, t))));
        }
    }
    let upper = upper.unwrap_or(ANY);
    Ok(u.types.intern(Type::Bounds(NOTHING, upper)))
}

/// Whether bounds text `bounds` bounds a type parameter by `Object` only.
pub fn bounded_by_object(bounds: &str) -> bool {
    bounds == OBJECT_BOUND
}

/// A reference to the class of internal name `internal`, applied to wildcards if it is a raw
/// type, one whose JDK class file has type parameters (dotty's `cook`).
pub fn cooked(u: &mut Universe, internal: &str) -> TypeId {
    let class = class_ref(u, internal);
    let jdk = u.jdk.as_mut().and_then(|jdk| jdk.class(internal));
    let Some(signature) = jdk.and_then(|c| c.signature.clone()) else { return class };
    let n = Sig::new(&signature).type_params(u).map_or(0, |params| params.len());
    if n == 0 {
        return class;
    }
    let wildcard = u.types.intern(Type::Bounds(NOTHING, ANY));
    u.types.intern(Type::Applied(class, vec![wildcard; n]))
}

/// A reference to the class of internal name `internal`, a nested class `A$B` as `A.B`.
fn class_ref(u: &mut Universe, internal: &str) -> TypeId {
    let (pkg, name) = internal.rsplit_once('/').unwrap_or(("", internal));
    let pkg = u.enter_package(&pkg.replace('/', "."));
    let mut prefix = u.types.intern(Type::Package(pkg));
    let mut parts: Vec<&str> = name.split('$').filter(|p| !p.is_empty()).collect();
    let last = parts.pop().unwrap_or(name);
    for outer in parts {
        prefix = u.types.intern(Type::TermRef(prefix, outer.to_owned()));
    }
    u.types.intern(Type::TypeRef(prefix, last.to_owned()))
}

/// The type of primitive descriptor `tag`.
fn primitive(u: &mut Universe, tag: u8) -> Option<TypeId> {
    let name = match tag {
        b'B' => "Byte",
        b'C' => "Char",
        b'D' => "Double",
        b'F' => "Float",
        b'I' => "Int",
        b'J' => "Long",
        b'S' => "Short",
        b'Z' => "Boolean",
        b'V' => "Unit",
        _ => return None,
    };
    Some(u.class_ref(&format!("scala.{name}")))
}
