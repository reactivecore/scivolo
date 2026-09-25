use super::Printer;
use crate::nir::{Param, Type, TypeId, Val, ValId};
use std::fmt::Write;

impl Printer<'_> {
    pub fn val(&mut self, v: ValId) {
        let pool = self.pool;
        let _ = match pool.val(v) {
            Val::True => write!(self.out, "true"),
            Val::False => write!(self.out, "false"),
            Val::Null => write!(self.out, "null"),
            Val::Unit => write!(self.out, "unit"),
            Val::Zero(ty) => {
                self.s("zero[");
                self.ty(*ty);
                write!(self.out, "]")
            }
            Val::Char(c) => write!(self.out, "char {c}"),
            Val::Byte(b) => write!(self.out, "byte {b}"),
            Val::Short(s) => write!(self.out, "short {s}"),
            Val::Int(i) => write!(self.out, "int {i}"),
            Val::Long(l) => write!(self.out, "long {l}"),
            Val::Float(f) => write!(self.out, "float {:?}", f32::from_bits(*f)),
            Val::Double(d) => write!(self.out, "double {:?}", f64::from_bits(*d)),
            Val::Size(s) => write!(self.out, "size {s}"),
            Val::Int128(hi, lo) => write!(self.out, "int128 {}", (i128::from(*hi) << 64) | i128::from(*lo)),
            Val::StructValue(vs) => {
                self.op_args("structvalue {", vs);
                write!(self.out, "}}")
            }
            Val::ArrayValue(ty, vs) => {
                self.s("arrayvalue ");
                self.ty(*ty);
                self.op_args(" {", vs);
                write!(self.out, "}}")
            }
            Val::ByteString(b) => write!(self.out, "c{:?}", String::from_utf8_lossy(b)),
            Val::Local(l, ty) => {
                self.param(&Param { id: *l, ty: *ty });
                Ok(())
            }
            Val::Global(g, ty) => {
                self.global(*g);
                self.s(" : ");
                self.ty(*ty);
                Ok(())
            }
            Val::Const(c) => {
                self.s("const ");
                self.val(*c);
                Ok(())
            }
            Val::String(s) => write!(self.out, "{:?}", pool.str(*s)),
            Val::Virtual(k) => write!(self.out, "virtual {k}"),
            Val::ClassOf(g) => {
                self.s("classOf[");
                self.global(*g);
                write!(self.out, "]")
            }
        };
    }

    pub fn ty(&mut self, t: TypeId) {
        let name = match self.pool.ty(t) {
            Type::Vararg => "...",
            Type::Bool => "bool",
            Type::Ptr => "ptr",
            Type::Char => "char",
            Type::Byte => "byte",
            Type::Short => "short",
            Type::Int => "int",
            Type::Long => "long",
            Type::Float => "float",
            Type::Double => "double",
            Type::Size => "size",
            Type::Int128 => "int128",
            Type::Null => "null",
            Type::Nothing => "nothing",
            Type::Virtual => "virtual",
            Type::Unit => "unit",
            composite => {
                self.composite(composite);
                return;
            }
        };
        self.s(name);
    }

    fn composite(&mut self, t: &Type) {
        match t {
            Type::ArrayValue(e, n) => {
                self.s("[");
                self.ty(*e);
                let _ = write!(self.out, " x {n}]");
            }
            Type::StructValue(tys) => {
                self.s("{");
                self.sep(tys, ", ", |p, t| p.ty(*t));
                self.s("}");
            }
            Type::Function(args, ret) => {
                self.s("(");
                self.sep(args, ", ", |p, t| p.ty(*t));
                self.s(") => ");
                self.ty(*ret);
            }
            Type::Var(t) => {
                self.s("var[");
                self.ty(*t);
                self.s("]");
            }
            Type::Array { elem, nullable } => {
                self.s(if *nullable { "array[" } else { "?array[" });
                self.ty(*elem);
                self.s("]");
            }
            Type::Ref { name, exact, nullable } => {
                self.s(if *exact { "!" } else { "" });
                self.s(if *nullable { "" } else { "?" });
                self.global(*name);
            }
            _ => {}
        }
    }
}
