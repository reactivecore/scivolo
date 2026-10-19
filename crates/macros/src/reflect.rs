//! The `quotes.reflect` operations of the overlay's `ReflectImpl` by name: terms are expression
//! numbers, types type numbers, symbols the frontend's symbol numbers (`-1` for none), constants
//! a kind, two words and a text.

use crate::host::Host;
use syntax::ast::constant::Constant;
use tasty::SymbolId;
use typer::inlining::Reflect;
use vm::Reflected;

type Answer = Result<Reflected, &'static str>;

impl Host {
    /// Runs `quotes.reflect` operation `op` on `args` and `text`.
    pub fn run_reflect(&mut self, op: &str, args: &[i32], text: &str) -> Answer {
        let arg = |i: usize| args.get(i).copied().ok_or("a quotes.reflect operation without its arguments");
        let numbers = |v: Vec<i32>| Ok((v, None));
        match op {
            "underlying" => {
                let t = self.tree(arg(0)? as u32)?;
                let u = self.reflect().underlying(t);
                numbers(vec![self.add_tree(u) as i32])
            }
            "constant" => {
                let t = self.tree(arg(0)? as u32)?;
                let c =
                    self.reflect().constant(t).ok_or("Literal.constant of a term other than a literal")?;
                Ok(encoded(c))
            }
            "literal" => {
                let c = decoded(arg(0)?, (arg(1)?, arg(2)?), text)?;
                let t = self.reflect().literal(c)?;
                numbers(vec![self.add_tree(t) as i32])
            }
            "typeSymbol" => {
                let t = self.type_of(arg(0)? as u32)?;
                numbers(vec![self.reflect().type_symbol(t).map_or(-1, |s| s.0 as i32)])
            }
            "companionModule" => {
                let Ok(s) = symbol(arg(0)?) else { return numbers(vec![-1]) };
                numbers(vec![self.reflect().companion_module(s).map_or(-1, |s| s.0 as i32)])
            }
            "declaredMethod" => {
                let Ok(s) = symbol(arg(0)?) else { return numbers(Vec::new()) };
                numbers(self.reflect().declared_methods(s, text).into_iter().map(|s| s.0 as i32).collect())
            }
            "ref" => {
                let t = self.reflect().ref_(symbol(arg(0)?)?)?;
                numbers(vec![self.add_tree(t) as i32])
            }
            "select" => {
                let qual = self.tree(arg(0)? as u32)?;
                let t = self.reflect().select(qual, symbol(arg(1)?)?)?;
                numbers(vec![self.add_tree(t) as i32])
            }
            "typeArgs" => {
                let t = self.type_of(arg(0)? as u32)?;
                let args = self.reflect().type_args(t);
                numbers(args.into_iter().map(|a| self.add_type(a) as i32).collect())
            }
            "appliedToTypes" => {
                let fun = self.tree(arg(0)? as u32)?;
                let types =
                    args[1..].iter().map(|&t| self.type_of(t as u32)).collect::<Result<Vec<_>, _>>()?;
                let t = self.reflect().applied_to_types(fun, &types)?;
                numbers(vec![self.add_tree(t) as i32])
            }
            _ => Err("a quotes.reflect operation the frontend does not know"),
        }
    }

    fn reflect(&mut self) -> Reflect<'_> {
        Reflect { u: &mut self.u, tpd: &mut self.tpd, names: &mut self.names }
    }
}

fn symbol(n: i32) -> Result<SymbolId, &'static str> {
    (n >= 0).then_some(SymbolId(n as u32)).ok_or("a quotes.reflect operation on Symbol.noSymbol")
}

/// Constant `c` as its kind, its value's two words and its text.
fn encoded(c: Constant) -> Reflected {
    let words = |bits: u64| vec![bits as u32 as i32, (bits >> 32) as u32 as i32];
    let (kind, bits) = match c {
        Constant::Int(i) => (1, i as i64 as u64),
        Constant::Long(l) => (2, l as u64),
        Constant::Boolean(b) => (3, b as u64),
        Constant::Char(c) => (4, c as u64),
        Constant::Double(d) => (5, d.to_bits()),
        Constant::Float(f) => (6, f.to_bits() as u64),
        Constant::String(ref s) => return (vec![7, 0, 0], Some(String::from_utf16_lossy(s))),
        Constant::Null => (8, 0),
        Constant::Unit => (9, 0),
        Constant::Byte(b) => (10, b as i64 as u64),
        Constant::Short(s) => (11, s as i64 as u64),
    };
    ([vec![kind], words(bits)].concat(), None)
}

/// The constant of kind `kind` with value words `lo` and `hi` and text `text`.
fn decoded(kind: i32, (lo, hi): (i32, i32), text: &str) -> Result<Constant, &'static str> {
    let bits = (lo as u32 as u64) | ((hi as u32 as u64) << 32);
    Ok(match kind {
        1 => Constant::Int(bits as i32),
        2 => Constant::Long(bits as i64),
        3 => Constant::Boolean(bits != 0),
        4 => Constant::Char(bits as u16),
        5 => Constant::Double(f64::from_bits(bits)),
        6 => Constant::Float(f32::from_bits(bits as u32)),
        7 => Constant::String(text.encode_utf16().collect()),
        8 => Constant::Null,
        9 => Constant::Unit,
        10 => Constant::Byte(bits as i8),
        11 => Constant::Short(bits as i16),
        _ => return Err("a constant of an unknown kind"),
    })
}
