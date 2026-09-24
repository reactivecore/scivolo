use super::Printer;
use crate::nir::{Cond, Inst, MemoryOrder, Next, Op, Param, Type, TypeId, Val, ValId};
use std::fmt::Write;

impl Printer<'_> {
    pub fn inst(&mut self, inst: &Inst) {
        match inst {
            Inst::Label(l, params) => {
                self.local(*l);
                if !params.is_empty() {
                    self.s("(");
                    self.sep(params, ", ", |p, param| p.param(param));
                    self.s(")");
                }
                self.s(":");
            }
            Inst::Let { id, op, unwind, scope } => {
                if *scope != 0 {
                    let _ = write!(self.out, "{scope}: ");
                }
                self.local(*id);
                self.s(" = ");
                self.op(op);
                self.unwind(unwind);
            }
            Inst::Ret(v) => self.op_args("ret ", &[*v]),
            Inst::Jump(n) => {
                self.s("jump ");
                self.next(n);
            }
            Inst::If(c, t, e) => {
                self.op_args("if ", &[*c]);
                self.branches(t, e);
            }
            Inst::LinktimeIf(cond, t, e) => {
                self.s("linktime if ");
                self.cond(cond);
                self.branches(t, e);
            }
            Inst::Switch(v, default, cases) => {
                self.op_args("switch ", &[*v]);
                self.s(" {");
                for c in cases {
                    self.s(" ");
                    self.next(c);
                }
                self.s(" default => ");
                self.next(default);
                self.s(" }");
            }
            Inst::Throw(v, unwind) => {
                self.op_args("throw ", &[*v]);
                self.unwind(unwind);
            }
            Inst::Unreachable(unwind) => {
                self.s("unreachable");
                self.unwind(unwind);
            }
        }
    }

    fn branches(&mut self, t: &Next, e: &Next) {
        self.s(" then ");
        self.next(t);
        self.s(" else ");
        self.next(e);
    }

    fn unwind(&mut self, n: &Next) {
        if !matches!(n, Next::None) {
            self.s(" ");
            self.next(n);
        }
    }

    fn cond(&mut self, c: &Cond) {
        match c {
            Cond::Simple { property, comp, value, .. } => {
                let _ =
                    write!(self.out, "{} {} ", self.pool.str(*property), format!("{comp:?}").to_lowercase());
                self.val(*value);
            }
            Cond::Complex { bin, left, right, .. } => {
                self.cond(left);
                let _ = write!(self.out, " {} ", format!("{bin:?}").to_lowercase());
                self.cond(right);
            }
        }
    }

    pub fn param(&mut self, p: &Param) {
        self.local(p.id);
        self.s(" : ");
        self.ty(p.ty);
    }

    fn typed(&mut self, name: &str, ty: TypeId) {
        self.s(name);
        self.s("[");
        self.ty(ty);
        self.s("] ");
    }

    fn op_args(&mut self, head: &str, vals: &[ValId]) {
        self.s(head);
        self.sep(vals, ", ", |p, v| p.val(*v));
    }

    fn order(&mut self, order: &Option<MemoryOrder>) {
        if let Some(o) = order {
            let _ = write!(self.out, " {}", format!("{o:?}").to_lowercase());
        }
    }

    fn op(&mut self, op: &Op) {
        match op {
            Op::Call { ty, ptr, args } => {
                self.typed("call", *ty);
                self.val(*ptr);
                self.s("(");
                self.sep(args, ", ", |p, v| p.val(*v));
                self.s(")");
            }
            Op::Load { ty, ptr, order } => {
                self.s(if order.is_some() { "atomic " } else { "" });
                self.typed("load", *ty);
                self.val(*ptr);
                self.order(order);
            }
            Op::Store { ty, ptr, value, order } => {
                self.s(if order.is_some() { "atomic " } else { "" });
                self.typed("store", *ty);
                self.op_args("", &[*ptr, *value]);
                self.order(order);
            }
            Op::Elem { ty, ptr, indexes } => {
                self.typed("elem", *ty);
                self.op_args("", &[*ptr]);
                self.s(", ");
                self.op_args("", indexes);
            }
            Op::Extract { aggr, indexes } | Op::Insert { aggr, indexes, .. } => {
                match op {
                    Op::Insert { value, .. } => self.op_args("insert ", &[*aggr, *value]),
                    _ => self.op_args("extract ", &[*aggr]),
                }
                let idx: Vec<String> = indexes.iter().map(i32::to_string).collect();
                let _ = write!(self.out, ", {}", idx.join(", "));
            }
            Op::Stackalloc { ty, n } => {
                self.typed("stackalloc", *ty);
                self.val(*n);
            }
            Op::Bin { bin, ty, l, r } => {
                self.typed(&format!("{bin:?}").to_lowercase(), *ty);
                self.op_args("", &[*l, *r]);
            }
            Op::Comp { comp, ty, l, r } => {
                self.typed(&format!("{comp:?}").to_lowercase(), *ty);
                self.op_args("", &[*l, *r]);
            }
            Op::Conv { conv, ty, value } => {
                self.typed(&format!("{conv:?}").to_lowercase(), *ty);
                self.val(*value);
            }
            Op::Fence(o) => self.s(&format!("fence {}", format!("{o:?}").to_lowercase())),
            Op::Classalloc { name, zone } => {
                self.s("classalloc ");
                self.global(*name);
                self.zone(zone);
            }
            Op::Fieldload { ty, obj, name } => {
                self.typed("fieldload", *ty);
                self.op_args("", &[*obj]);
                self.s(", ");
                self.global(*name);
            }
            Op::Fieldstore { ty, obj, name, value } => {
                self.typed("fieldstore", *ty);
                self.op_args("", &[*obj]);
                self.s(", ");
                self.global(*name);
                self.op_args(", ", &[*value]);
            }
            Op::Field { obj, name } => {
                self.op_args("field ", &[*obj]);
                self.s(", ");
                self.global(*name);
            }
            Op::Method { obj, sig } | Op::Dynmethod { obj, sig } => {
                let head = if matches!(op, Op::Method { .. }) { "method " } else { "dynmethod " };
                self.op_args(head, &[*obj]);
                self.s(", ");
                self.sig(self.pool.str(*sig));
            }
            Op::Module(g) => {
                self.s("module ");
                self.global(*g);
            }
            Op::As { ty, obj } => self.ty_val("as", *ty, *obj),
            Op::Is { ty, obj } => self.ty_val("is", *ty, *obj),
            Op::Box { ty, obj } => self.ty_val("box", *ty, *obj),
            Op::Unbox { ty, obj } => self.ty_val("unbox", *ty, *obj),
            Op::Copy(v) => self.op_args("copy ", &[*v]),
            Op::SizeOf(ty) => self.typed("sizeOf", *ty),
            Op::AlignmentOf(ty) => self.typed("alignmentOf", *ty),
            Op::Var(ty) => {
                self.typed("var", *ty);
                self.out.pop();
            }
            Op::Varload(v) => self.op_args("varload ", &[*v]),
            Op::Varstore { slot, value } => self.op_args("varstore ", &[*slot, *value]),
            Op::Arrayalloc { ty, init, zone } => {
                self.ty_val("arrayalloc", *ty, *init);
                self.zone(zone);
            }
            Op::Arrayload { ty, arr, idx } => {
                self.typed("arrayload", *ty);
                self.op_args("", &[*arr, *idx]);
            }
            Op::Arraystore { ty, arr, idx, value } => {
                self.typed("arraystore", *ty);
                self.op_args("", &[*arr, *idx, *value]);
            }
            Op::Arraylength(v) => self.op_args("arraylength ", &[*v]),
        }
    }

    fn ty_val(&mut self, name: &str, ty: TypeId, v: ValId) {
        self.typed(name, ty);
        self.val(v);
    }

    fn zone(&mut self, zone: &Option<ValId>) {
        if let Some(z) = zone {
            self.op_args(" inZone ", &[*z]);
        }
    }

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
