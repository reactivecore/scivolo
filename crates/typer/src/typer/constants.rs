//! Constants as dotty's `Constants` has them: tags, values in other value classes, and
//! `convertTo`.

use super::Typer;
use syntax::ast::constant::Constant as C;
use tasty::tags::*;
use tasty::{Kind, SymbolId, Type, TypeId, TypeVars};

pub const BOOLEAN: u8 = 2;
pub const BYTE: u8 = 3;
const SHORT: u8 = 4;
const CHAR: u8 = 5;
pub const INT: u8 = 6;
pub const LONG: u8 = 7;
pub const FLOAT: u8 = 8;
pub const DOUBLE: u8 = 9;
pub const STRING: u8 = 10;
pub const NULL: u8 = 11;

/// dotty's constant tag of `c`, ordered as numeric widening goes.
pub fn tag(c: &C) -> u8 {
    match c {
        C::Unit => 1,
        C::Boolean(_) => BOOLEAN,
        C::Byte(_) => BYTE,
        C::Short(_) => SHORT,
        C::Char(_) => CHAR,
        C::Int(_) => INT,
        C::Long(_) => LONG,
        C::Float(_) => FLOAT,
        C::Double(_) => DOUBLE,
        C::String(_) => STRING,
        C::Null => NULL,
    }
}

pub fn is_numeric(c: &C) -> bool {
    (BYTE..=DOUBLE).contains(&tag(c))
}

pub fn long_value(c: &C) -> i64 {
    match *c {
        C::Byte(v) => v as i64,
        C::Short(v) => v as i64,
        C::Char(v) => v as i64,
        C::Int(v) => v as i64,
        C::Long(v) => v,
        C::Float(v) => v as i64,
        C::Double(v) => v as i64,
        _ => 0,
    }
}

pub fn int_value(c: &C) -> i32 {
    match *c {
        C::Float(v) => v as i32,
        C::Double(v) => v as i32,
        _ => long_value(c) as i32,
    }
}

pub fn float_value(c: &C) -> f32 {
    match *c {
        C::Float(v) => v,
        C::Double(v) => v as f32,
        _ => long_value(c) as f32,
    }
}

pub fn double_value(c: &C) -> f64 {
    match *c {
        C::Float(v) => v as f64,
        C::Double(v) => v,
        _ => long_value(c) as f64,
    }
}

/// The full names of the classes of the constants by tag, the primitive value classes first.
const CLASSES: [&str; 11] = [
    "scala.Unit",
    "scala.Boolean",
    "scala.Byte",
    "scala.Short",
    "scala.Char",
    "scala.Int",
    "scala.Long",
    "scala.Float",
    "scala.Double",
    "java.lang.String",
    "scala.Null",
];

/// `c` converted to the class of full name `target` (dotty's `convertTo` after `classBound`).
pub fn convert_to(c: &C, target: &str) -> Option<C> {
    let t = tag(c);
    let int = (BYTE..=INT).contains(&t).then(|| int_value(c));
    Some(match target {
        _ if target == CLASSES[t as usize - 1] => c.clone(),
        "scala.Byte" => C::Byte(i8::try_from(int?).ok()?),
        "scala.Short" => C::Short(i16::try_from(int?).ok()?),
        "scala.Char" => C::Char(u16::try_from(int?).ok()?),
        "scala.Int" => C::Int(int?),
        "scala.Long" if (BYTE..=LONG).contains(&t) => C::Long(long_value(c)),
        "scala.Float" if (BYTE..=FLOAT).contains(&t) => C::Float(float_value(c)),
        "scala.Double" if is_numeric(c) => C::Double(double_value(c)),
        _ => return None,
    })
}

/// The constant of a constant type of tag `tag` and bits `bits`.
pub fn of_type(tag: u8, bits: i64) -> Option<C> {
    Some(match tag {
        UNITconst => C::Unit,
        TRUEconst => C::Boolean(true),
        FALSEconst => C::Boolean(false),
        BYTEconst => C::Byte(bits as i8),
        SHORTconst => C::Short(bits as i16),
        CHARconst => C::Char(bits as u16),
        INTconst => C::Int(bits as i32),
        LONGconst => C::Long(bits),
        FLOATconst => C::Float(f32::from_bits(bits as u32)),
        DOUBLEconst => C::Double(f64::from_bits(bits as u64)),
        NULLconst => C::Null,
        _ => return None,
    })
}

impl Typer<'_> {
    /// `c` converted to conform to `pt` (dotty's `Constant.convertTo`).
    pub fn convert_constant(&mut self, c: &C, pt: TypeId) -> Option<C> {
        if self.is_singleton(pt) {
            return None;
        }
        let target = self.class_bound(pt)?;
        convert_to(c, &self.u.full_name(target))
    }

    /// The class of `pt` through aliases, type variables, `| Null` and lower bounds of abstract
    /// types (dotty's `classBound(pt).typeSymbol`).
    pub fn class_bound(&mut self, pt: TypeId) -> Option<SymbolId> {
        self.class_bound_within(pt, 8)
    }

    fn class_bound_within(&mut self, pt: TypeId, depth: u32) -> Option<SymbolId> {
        let depth = depth.checked_sub(1)?;
        let pt = self.u.dealias(pt);
        match *self.u.types().get(pt) {
            Type::Var(v) => {
                if let Some(inst) = self.constraint.instance(v) {
                    return self.class_bound_within(inst, depth);
                }
                let (lo, hi) = self.constraint.bounds(v);
                match self.u.class_symbol(hi) {
                    Some(c) if CLASSES[..9].contains(&&*self.u.full_name(c)) => Some(c),
                    _ => self.class_bound_within(lo, depth),
                }
            }
            Type::Or(a, b, _) => {
                let null = self.u.builtins().null;
                match (self.u.class_symbol(a), self.u.class_symbol(b)) {
                    (_, Some(c)) if c == null => self.class_bound_within(a, depth),
                    (Some(c), _) if c == null => self.class_bound_within(b, depth),
                    _ => None,
                }
            }
            Type::Sym(_, s) if matches!(self.u.symbol(s).kind, Kind::Type | Kind::TypeParam) => {
                let info = self.u.info(s);
                match *self.u.types().get(info) {
                    Type::Bounds(lo, _) => self.class_bound_within(lo, depth),
                    _ => None,
                }
            }
            _ => self.direct_class(pt),
        }
    }
}
