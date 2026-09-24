use crate::program::{ClassId, Program};
use std::fmt::Write;
use std::rc::Rc;

#[derive(Debug, Clone)]
pub enum Value {
    Unit,
    Null,
    Bool(bool),
    Int(i32),
    Long(i64),
    Double(f64),
    Str(Rc<str>),
    Obj(Rc<Obj>),
}

#[derive(Debug)]
pub struct Obj {
    pub class: ClassId,
    pub fields: Box<[Value]>,
}

/// A thrown Scala exception, reduced to its message.
#[derive(Debug)]
pub struct Throw(pub Box<String>);

pub type R = Result<Value, Throw>;

pub fn throw(msg: impl Into<String>) -> R {
    Err(Throw(Box::new(msg.into())))
}

impl Value {
    #[inline(always)]
    pub fn bool(&self) -> bool {
        match self {
            Value::Bool(b) => *b,
            v => unreachable!("expected Boolean, got {v:?}"),
        }
    }

    #[inline(always)]
    pub fn int(&self) -> i32 {
        match self {
            Value::Int(x) => *x,
            v => unreachable!("expected Int, got {v:?}"),
        }
    }

    #[inline(always)]
    pub fn long(&self) -> i64 {
        match self {
            Value::Long(x) => *x,
            v => unreachable!("expected Long, got {v:?}"),
        }
    }

    #[inline(always)]
    pub fn double(&self) -> f64 {
        match self {
            Value::Double(x) => *x,
            v => unreachable!("expected Double, got {v:?}"),
        }
    }

    pub fn obj(&self) -> Result<&Obj, Throw> {
        match self {
            Value::Obj(o) => Ok(o),
            _ => Err(Throw(Box::new("java.lang.NullPointerException".into()))),
        }
    }

    /// Appends the value's `toString` as the JVM would print it.
    pub fn show(&self, prog: &Program, out: &mut String) {
        match self {
            Value::Unit => out.push_str("()"),
            Value::Null => out.push_str("null"),
            Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
            Value::Int(x) => _ = write!(out, "{x}"),
            Value::Long(x) => _ = write!(out, "{x}"),
            Value::Double(x) => show_double(*x, out),
            Value::Str(s) => out.push_str(s),
            Value::Obj(o) => {
                out.push_str(prog.class(o.class).name.trim_end_matches('$'));
                out.push('(');
                for (i, f) in o.fields.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    f.show(prog, out);
                }
                out.push(')');
            }
        }
    }
}

fn show_double(x: f64, out: &mut String) {
    if x.is_nan() {
        out.push_str("NaN");
    } else if x.is_infinite() {
        out.push_str(if x > 0.0 { "Infinity" } else { "-Infinity" });
    } else if x == 0.0 || (1e-3..1e7).contains(&x.abs()) {
        let s = format!("{x:?}");
        out.push_str(&s);
    } else {
        let s = format!("{x:e}");
        let (m, e) = s.split_once('e').unwrap_or((&s, "0"));
        out.push_str(m);
        if !m.contains('.') {
            out.push_str(".0");
        }
        out.push('E');
        out.push_str(e);
    }
}
