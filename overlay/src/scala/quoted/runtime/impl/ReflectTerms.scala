// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); changed:
//   compiler/src/scala/quoted/runtime/impl/QuotesImpl.scala
package scala.quoted.runtime.impl

import scala.quoted.Expr

/** The trees and terms of `quotes.reflect`. */
trait ReflectTermSupport { this: QuotesImpl =>

trait ReflectTerms extends reflectModule { self: reflect.type =>

  given TreeMethods: TreeMethods = methods("TreeMethodsImpl")
  given TermMethods: TermMethods = methods("TermMethodsImpl")
  given LiteralMethods: LiteralMethods = methods("LiteralMethodsImpl")

  object Ref extends RefModule {
    def term(tp: TermRef): Ref = throw new Unsupported("Ref.term")
    def apply(sym: Symbol): Ref = termOf(ReflectOps.one("ref", ReflectOps.id(sym))).asInstanceOf[Ref]
  }

  object Literal extends LiteralModule {
    def apply(constant: Constant): Literal = {
      val c = constant.asInstanceOf[ConstantImpl]
      ReflectOps.run("literal", Array(c.kind, c.lo, c.hi), c.text)
      termOf(QuoteHost.result(0)).asInstanceOf[Literal]
    }
    def copy(original: Tree)(constant: Constant): Literal = throw new Unsupported("Literal.copy")
    def unapply(x: Literal): Some[Constant] = Some(x.constant)
  }

  abstract class TreeMethodsImpl extends TreeMethods {
    extension (self: Tree) {
      def asExpr: Expr[Any] = self.asInstanceOf[Expr[Any]]
      def pos: Position = ReflectOps.position("pos", ReflectOps.id(self)).asInstanceOf[Position]
    }
  }

  abstract class TermMethodsImpl extends TermMethods {
    extension (self: Term) {
      def underlying: Term = ReflectOps.term(ReflectOps.one("underlying", ReflectOps.id(self))).asInstanceOf[Term]
      def select(sym: Symbol): Select =
        ReflectOps.term(ReflectOps.one("select", ReflectOps.id(self), ReflectOps.id(sym))).asInstanceOf[Select]
      def appliedToType(targ: TypeRepr): Term = self.appliedToTypes(List(targ))
      def appliedToTypes(targs: List[TypeRepr]): Term = {
        val args = ReflectOps.id(self) :: targs.map(ReflectOps.id)
        ReflectOps.term(ReflectOps.one("appliedToTypes", args*)).asInstanceOf[Term]
      }
    }
  }

  abstract class LiteralMethodsImpl extends LiteralMethods {
    extension (self: Literal) def constant: Constant = {
      ReflectOps.run("constant", Array(ReflectOps.id(self)), null)
      new ConstantImpl(QuoteHost.result(0), QuoteHost.result(1), QuoteHost.result(2), QuoteHost.resultText())
        .asInstanceOf[Constant]
    }
  }

  private def methods[T](name: String): T =
    QuoteHost.allocate("scala.quoted.runtime.impl.ReflectTermSupport$ReflectTerms$" + name).asInstanceOf[T]

  private def termOf(id: Int): Term = ReflectOps.term(id).asInstanceOf[Term]
}
}
