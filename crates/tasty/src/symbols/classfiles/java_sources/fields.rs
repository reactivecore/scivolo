// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/parsing/JavaParsers.scala
//   compiler/src/dotty/tools/dotc/core/Constants.scala
//! The fields of a Java declaration, a static final one's constant value from its literal.

use super::parser::Parser;
use super::scanner::Token;
use super::syntax::{Class, Field, Type};
use classfile::{ACC_FINAL, ACC_STATIC, ConstantValue};

impl Parser {
    /// Reads the fields `name, …` of one declaration of type `ty` up to its `;`, a constant
    /// value for a static final one of a literal (dotty's `fieldDecls`, `varDecl`). A name after
    /// a comma is a field only if `=`, `;` or another such name follows it; otherwise the comma
    /// was inside an initializer (`new HashMap<K, V>()`), which is skipped.
    pub fn fields(&mut self, class: &mut Class, access: u16, ty: Type, name: String) -> Result<(), String> {
        self.field(class, access, &ty, name)?;
        let mut maybe = Vec::new();
        while self.eat(",") {
            let Token::Ident(name) = self.peek().clone() else {
                maybe.clear();
                self.skip_to_comma_or_semi()?;
                continue;
            };
            self.bump();
            if self.is("=") || self.is(";") {
                class.fields.append(&mut maybe);
                self.field(class, access, &ty, name)?;
            } else if self.is(",") {
                maybe.push(Field { access, name, ty: ty.clone(), constant: None });
            } else {
                maybe.clear();
                self.skip_to_comma_or_semi()?;
            }
        }
        class.fields.append(&mut maybe);
        self.expect(";")
    }

    /// Reads field `name` of type `ty` from its array brackets to the end of its initializer.
    fn field(&mut self, class: &mut Class, access: u16, ty: &Type, name: String) -> Result<(), String> {
        let mut ty = ty.clone();
        while self.eat("[") {
            self.expect("]")?;
            ty = Type::Array(Box::new(ty));
        }
        let mut constant = None;
        if self.eat("=") {
            if access & (ACC_STATIC | ACC_FINAL) == ACC_STATIC | ACC_FINAL {
                constant = literal_constant(self, &ty);
            }
            self.skip_to_comma_or_semi()?;
        }
        class.fields.push(Field { access, name, ty, constant });
        Ok(())
    }
}

/// Determines the constant value of the initializer at the cursor of a static final field of
/// type `ty`: a literal, `-` or `!` before it, alone up to the `;`, converted to `ty` (a `String`
/// for a string); none otherwise or where it does not convert (dotty's `tryConstant`,
/// `forConst`, `Constant.convertTo`). The cursor moves past the literal.
fn literal_constant(p: &mut Parser, ty: &Type) -> Option<ConstantValue> {
    let negate = p.eat("-") || p.eat("!");
    let literal = match p.peek() {
        t @ (Token::Int(_)
        | Token::Long(_)
        | Token::Float(_)
        | Token::Double(_)
        | Token::Char(_)
        | Token::Str(_)) => t.clone(),
        Token::Ident(w) if w == "true" || w == "false" => Token::Ident(w.clone()),
        _ => return None,
    };
    p.bump();
    if !p.is(";") {
        return None;
    }
    match (literal, ty) {
        (Token::Str(s), Type::Named(parts)) if !negate && parts.len() == 1 && parts[0].0 == "String" => {
            Some(ConstantValue::String(s))
        }
        (Token::Ident(w), Type::Primitive('Z')) => {
            Some(ConstantValue::Int(i32::from((w == "true") != negate)))
        }
        (literal, &Type::Primitive(letter)) => numeric(literal, negate, letter),
        _ => None,
    }
}

/// The numeric `literal`, negated if `negate`, converted to the primitive type of descriptor
/// letter `letter`: an integral one in its range, a widening otherwise.
fn numeric(literal: Token, negate: bool, letter: char) -> Option<ConstantValue> {
    let sign = |v: i64| if negate { v.wrapping_neg() } else { v };
    let (integral, long, float) = match literal {
        Token::Int(v) => (Some(sign(v)), false, None),
        Token::Char(c) if !negate => (Some(i64::from(u32::from(c))), false, None),
        Token::Long(v) => (Some(sign(v)), true, None),
        Token::Float(v) => (None, false, Some((if negate { -v } else { v }, true))),
        Token::Double(v) => (None, false, Some((if negate { -v } else { v }, false))),
        _ => return None,
    };
    if let Some(v) = integral {
        let int_range = !long && i32::try_from(v).is_ok();
        let fits = |lo: i64, hi: i64| int_range && (lo..=hi).contains(&v);
        return match letter {
            'B' if fits(i64::from(i8::MIN), i64::from(i8::MAX)) => Some(ConstantValue::Int(v as i32)),
            'S' if fits(i64::from(i16::MIN), i64::from(i16::MAX)) => Some(ConstantValue::Int(v as i32)),
            'C' if fits(0, i64::from(u16::MAX)) => Some(ConstantValue::Int(v as i32)),
            'I' if int_range => Some(ConstantValue::Int(v as i32)),
            'J' => Some(ConstantValue::Long(v)),
            'F' => Some(ConstantValue::Float((v as f32).to_bits())),
            'D' => Some(ConstantValue::Double((v as f64).to_bits())),
            _ => None,
        };
    }
    let (v, is_float) = float?;
    match letter {
        'F' if is_float => Some(ConstantValue::Float((v as f32).to_bits())),
        'D' => Some(ConstantValue::Double(if is_float { f64::from(v as f32) } else { v }.to_bits())),
        _ => None,
    }
}
