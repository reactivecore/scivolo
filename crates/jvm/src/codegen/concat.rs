// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/backend/jvm/BCodeBodyBuilder.scala
//   compiler/src/dotty/tools/backend/jvm/BCodeIdiomatic.scala
//! String concatenation as `invokedynamic` of `StringConcatFactory.makeConcatWithConstants`
//! (dotty's `genStringConcat`, `liftStringConcat`, `genIndyStringConcat`).

use super::Gen;
use super::body::{Body, Operand};
use super::closures::BOOTSTRAP;
use super::descriptors::{descriptor, method_type};
use classfile::{Constant, Handle, Invoke};
use lower::Result;
use names::nme;
use syntax::ast::constant::Constant as Literal;
use typer::tpd::{TNodeId, Tree};

/// The most argument slots of one `makeConcatWithConstants` call site.
const MAX_SLOTS: i32 = 200;
/// The recipe's hole for an argument and for a constant.
const TAG_ARG: u16 = 1;
const TAG_CONST: u16 = 2;

impl Gen<'_, '_> {
    /// Whether `t` calls `String.+` or a number's `+` of a `String` (dotty's `CONCAT`).
    pub fn is_concat(&mut self, t: TNodeId) -> bool {
        let Tree::Apply { fun, .. } = *self.l.tpd.tree(t) else { return false };
        let Some(m) = self.l.tpd.sym(fun) else { return false };
        let Some(owner) = self.l.u.symbol(m).owner.map(|o| self.l.u.binary_name(o)) else { return false };
        if self.l.u.symbol(m).name != nme::ADD {
            return false;
        }
        let numbers = ["Byte", "Short", "Char", "Int", "Long", "Float", "Double"];
        let number = owner.strip_prefix("scala.").is_some_and(|n| numbers.contains(&n));
        let string = tasty::Erased::Class("java.lang.String".into());
        owner == "java.lang.String" || number && self.l.u.erased_method(m).0.first() == Some(&string)
    }

    /// The operands of the chain of `String.+` calls `t`, from left to right.
    fn concat_operands(&mut self, t: TNodeId) -> Vec<TNodeId> {
        if self.is_concat(t)
            && let Tree::Apply { fun, args } = *self.l.tpd.tree(t)
            && let Tree::Select { qualifier, .. } = *self.l.tpd.tree(fun)
        {
            let mut operands = self.concat_operands(qualifier);
            operands.extend(self.l.tpd.items(args));
            return operands;
        }
        vec![t]
    }

    /// String concatenation `t`.
    pub fn concat(&mut self, b: &mut Body, t: TNodeId) -> Result<i32> {
        let operands = self.concat_operands(t);
        let empty = |g: &Self, t| matches!(g.l.tpd.tree(t), Tree::Literal { value: Literal::String(s) } if s.is_empty());
        if let [first, arg] = operands[..]
            && empty(self, first)
        {
            self.expr(b, arg)?;
            let d = "(Ljava/lang/Object;)Ljava/lang/String;";
            b.code.invoke(Invoke::Static, b.pool, "java/lang/String", nme::VALUE_OF, d);
            return Ok(1);
        }
        let mut operands: Vec<TNodeId> = operands.into_iter().filter(|&o| !empty(self, o)).collect();
        for o in &mut operands {
            if let Tree::Apply { fun, args } = *self.l.tpd.tree(*o)
                && let (Some(m), &[value]) = (self.l.tpd.sym(fun), self.l.tpd.items(args))
                && self.l.is_box(m)
                && lower::erased_type(self.l, value) != tasty::Erased::Unit
            {
                *o = value;
            }
        }
        let (mut recipe, mut types, mut constants) = (Vec::new(), String::new(), Vec::new());
        let (mut slots, mut concats) = (0, 1);
        for o in operands {
            let desc = descriptor(&lower::erased_type(self.l, o));
            let size = classfile::value_slots(&desc);
            if slots + size >= MAX_SLOTS {
                concat_call(b, &recipe, &types, &constants);
                b.operand(b.code.stack() - 1, 1, Operand::Desc("L"));
                concats += 1;
                slots = 0;
                (recipe, types, constants) = (Vec::new(), String::new(), Vec::new());
            }
            match self.l.tpd.tree(o) {
                Tree::Literal { value: Literal::String(s) }
                    if s.contains(&TAG_ARG) || s.contains(&TAG_CONST) =>
                {
                    slots += size;
                    recipe.push(TAG_CONST);
                    constants.push(s.to_vec());
                }
                Tree::Literal { value: Literal::String(s) } => recipe.extend_from_slice(s),
                _ => {
                    slots += size;
                    recipe.push(TAG_ARG);
                    types.push_str(&desc);
                    self.expr(b, o)?;
                }
            }
        }
        concat_call(b, &recipe, &types, &constants);
        if concats > 1 {
            let recipe = vec![TAG_ARG; concats];
            concat_call(b, &recipe, &"Ljava/lang/String;".repeat(concats), &[]);
        }
        Ok(1)
    }
}

/// The call of `makeConcatWithConstants` with `recipe`, arguments of descriptors `types` on the
/// stack, and `constants`.
fn concat_call(b: &mut Body, recipe: &[u16], types: &str, constants: &[Vec<u16>]) {
    let d = format!("({BOOTSTRAP}Ljava/lang/String;[Ljava/lang/Object;)Ljava/lang/invoke/CallSite;");
    let bootstrap =
        Handle::static_method("java/lang/invoke/StringConcatFactory", "makeConcatWithConstants", &d);
    let args: Vec<Constant> =
        std::iter::once(recipe).chain(constants.iter().map(Vec::as_slice)).map(Constant::String).collect();
    let desc = method_type(types, "Ljava/lang/String;");
    b.code.invoke_dynamic(b.pool, ("makeConcatWithConstants", &desc), &bootstrap, &args);
}
