// Derived from Scala Native 0.5.12 (Apache-2.0, see native/LICENSE-scala-native.md); translated to Rust and changed:
//   nir/src/main/scala/scala/scalanative/nir/Mangle.scala
//   nir/src/main/scala/scala/scalanative/nir/Sig.scala
//! Member signatures and their mangled form (Scala Native's `Sig` and `Mangle`).

use crate::nir::{Global, GlobalId, StrId, Type, TypeId};
use crate::pool::Pool;

pub enum Scope {
    Public,
    PublicStatic,
    Private(GlobalId),
    PrivateStatic(GlobalId),
}

pub enum Sig {
    Field(String, Scope),
    Ctor(Vec<TypeId>),
    Clinit,
    Method(String, Vec<TypeId>, Scope),
    Extern(String),
    Generated(String),
    Proxy(String, Vec<TypeId>),
}

impl Pool {
    /// The interned mangled form of `sig`.
    pub fn sig(&mut self, sig: &Sig) -> StrId {
        let mut out = String::new();
        match sig {
            Sig::Field(id, scope) => {
                out.push('F');
                ident(&mut out, id);
                self.scope(&mut out, scope);
            }
            Sig::Ctor(types) => {
                out.push('R');
                types.iter().for_each(|&t| self.mangle_type(&mut out, t));
                out.push('E');
            }
            Sig::Clinit => out.push_str("IE"),
            Sig::Method(id, types, scope) => {
                out.push('D');
                ident(&mut out, id);
                types.iter().for_each(|&t| self.mangle_type(&mut out, t));
                out.push('E');
                self.scope(&mut out, scope);
            }
            Sig::Extern(id) => {
                out.push('C');
                ident(&mut out, id);
            }
            Sig::Generated(id) => {
                out.push('G');
                ident(&mut out, id);
            }
            Sig::Proxy(id, types) => {
                out.push('P');
                ident(&mut out, id);
                types.iter().for_each(|&t| self.mangle_type(&mut out, t));
                out.push('E');
            }
        }
        StrId(self.strings.intern_str(&out))
    }

    /// The member `sig` of top-level `owner`.
    pub fn member(&mut self, owner: GlobalId, sig: &Sig) -> GlobalId {
        let sig = self.sig(sig);
        GlobalId(self.globals.intern(Global::Member(owner, sig)))
    }

    /// The top-level global named `id`.
    pub fn top(&mut self, id: &str) -> GlobalId {
        let s = StrId(self.strings.intern_str(id));
        GlobalId(self.globals.intern(Global::Top(s)))
    }

    fn scope(&self, out: &mut String, scope: &Scope) {
        match scope {
            Scope::Public => out.push('O'),
            Scope::PublicStatic => out.push('o'),
            Scope::Private(g) => {
                out.push('P');
                out.push_str(&self.mangle(*g));
            }
            Scope::PrivateStatic(g) => {
                out.push('p');
                out.push_str(&self.mangle(*g));
            }
        }
    }

    fn mangle_type(&self, out: &mut String, t: TypeId) {
        match self.ty(t) {
            Type::Vararg => out.push('v'),
            Type::Ptr => out.push_str("R_"),
            Type::Bool => out.push('z'),
            Type::Char => out.push('c'),
            Type::Byte => out.push('b'),
            Type::Short => out.push('s'),
            Type::Int => out.push('i'),
            Type::Long => out.push('j'),
            Type::Size => out.push('w'),
            Type::Float => out.push('f'),
            Type::Double => out.push('d'),
            Type::Null => out.push('l'),
            Type::Nothing => out.push('n'),
            Type::Unit => out.push('u'),
            Type::ArrayValue(elem, n) => {
                out.push('A');
                self.mangle_type(out, *elem);
                out.push_str(&n.to_string());
                out.push('_');
            }
            Type::StructValue(tys) => {
                out.push('S');
                tys.iter().for_each(|&t| self.mangle_type(out, t));
                out.push('E');
            }
            Type::Function(args, ret) => {
                out.push('R');
                args.iter().for_each(|&t| self.mangle_type(out, t));
                self.mangle_type(out, *ret);
                out.push('E');
            }
            Type::Array { elem, nullable } => {
                out.push_str(if *nullable { "LA" } else { "A" });
                self.mangle_type(out, *elem);
                out.push('_');
            }
            Type::Ref { name, exact, nullable } => {
                out.push_str(if *nullable { "L" } else { "" });
                out.push_str(if *exact { "X" } else { "" });
                ident(out, self.top_name(*name));
            }
            t => unreachable!("no mangled form: {t:?}"),
        }
    }
}

/// `id` as a mangled identifier: its length in UTF-16 units, a `-` if it starts with a digit or
/// `-`, then itself.
pub fn ident(out: &mut String, id: &str) {
    out.push_str(&id.encode_utf16().count().to_string());
    if id.starts_with(|c: char| c.is_ascii_digit() || c == '-') {
        out.push('-');
    }
    out.push_str(id);
}
