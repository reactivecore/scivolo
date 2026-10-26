// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); changed:
//   compiler/src/scala/quoted/runtime/impl/QuotesImpl.scala
package scala.quoted.runtime.impl

import scala.reflect.TypeTest

/** The control structures of `quotes.reflect`: `super`, `new`, assignments, conditionals,
 *  matches, loops, returns and `try`.
 */
trait ReflectControlSupport { this: QuotesImpl =>

trait ReflectControl extends reflectModule { self: reflect.type =>

  given SuperTypeTest: TypeTest[Tree, Super] = test(21)
  given IfTypeTest: TypeTest[Tree, If] = test(24)
  given MatchTypeTest: TypeTest[Tree, Match] = test(25)
  given SummonFromTypeTest: TypeTest[Tree, SummonFrom] = test(26)
  given ReturnTypeTest: TypeTest[Tree, Return] = test(27)
  given WhileTypeTest: TypeTest[Tree, While] = test(28)
  given TryTypeTest: TypeTest[Tree, Try] = test(29)
  given CaseDefTypeTest: TypeTest[Tree, CaseDef] = test(30)
  given AssignTypeTest: TypeTest[Tree, Assign] = test(23)
  given NewTypeTest: TypeTest[Tree, New] = test(22)

  given SuperMethods: SuperMethods = methods("SuperMethodsImpl")
  given IfMethods: IfMethods = methods("IfMethodsImpl")
  given MatchMethods: MatchMethods = methods("MatchMethodsImpl")
  given SummonFromMethods: SummonFromMethods = methods("SummonFromMethodsImpl")
  given ReturnMethods: ReturnMethods = methods("ReturnMethodsImpl")
  given WhileMethods: WhileMethods = methods("WhileMethodsImpl")
  given TryMethods: TryMethods = methods("TryMethodsImpl")
  given CaseDefMethods: CaseDefMethods = methods("CaseDefMethodsImpl")
  given AssignMethods: AssignMethods = methods("AssignMethodsImpl")
  given NewMethods: NewMethods = methods("NewMethodsImpl")

  object Assign extends AssignModule {
    def apply(lhs: Term, rhs: Term): Assign = built("assign", lhs, rhs).asInstanceOf[Assign]
    def copy(original: Tree)(lhs: Term, rhs: Term): Assign = ReflectOps.copy(original, null, lhs, rhs).asInstanceOf[Assign]
    def unapply(x: Assign): (Term, Term) = (x.lhs, x.rhs)
  }

  object New extends NewModule {
    def apply(tpt: TypeTree): New = built("new", tpt).asInstanceOf[New]
    def copy(original: Tree)(tpt: TypeTree): New = ReflectOps.copy(original, null, tpt).asInstanceOf[New]
    def unapply(x: New): Some[TypeTree] = Some(x.tpt)
  }

  object Super extends SuperModule {
    def apply(qual: Term, mix: Option[String]): Super = refused("Super.apply")
    def copy(original: Tree)(qual: Term, mix: Option[String]): Super =
      ReflectOps.copy(original, mix.orNull, qual).asInstanceOf[Super]
    def unapply(x: Super): (Term, Option[String]) = (x.qualifier, x.id)
  }

  object If extends IfModule {
    def apply(cond: Term, thenp: Term, elsep: Term): If = built("if", cond, thenp, elsep).asInstanceOf[If]
    def copy(original: Tree)(cond: Term, thenp: Term, elsep: Term): If =
      ReflectOps.copy(original, null, cond, thenp, elsep).asInstanceOf[If]
    def unapply(tree: If): (Term, Term, Term) = (tree.cond, tree.thenp, tree.elsep)
  }

  object Match extends MatchModule {
    def apply(selector: Term, cases: List[CaseDef]): Match = built("match", selector :: cases*).asInstanceOf[Match]
    def copy(original: Tree)(selector: Term, cases: List[CaseDef]): Match =
      ReflectOps.copy(original, null, selector, cases).asInstanceOf[Match]
    def unapply(x: Match): (Term, List[CaseDef]) = (x.scrutinee, x.cases)
  }

  object SummonFrom extends SummonFromModule {
    def apply(cases: List[CaseDef]): SummonFrom = refused("SummonFrom.apply")
    def copy(original: Tree)(cases: List[CaseDef]): SummonFrom = ReflectOps.copy(original, null, cases).asInstanceOf[SummonFrom]
    def unapply(x: SummonFrom): Some[List[CaseDef]] = Some(x.cases)
  }

  object Return extends ReturnModule {
    def apply(expr: Term, from: Symbol): Return = refused("Return.apply")
    def copy(original: Tree)(expr: Term, from: Symbol): Return = {
      val s = ReflectOps.shape(original)
      if (from != s.child(1).asInstanceOf[Tree].symbol) refused("Return.copy from another method")
      ReflectOps.copy(original, null, expr, s.child(1)).asInstanceOf[Return]
    }
    def unapply(x: Return): (Term, Symbol) = (x.expr, x.from)
  }

  object While extends WhileModule {
    def apply(cond: Term, body: Term): While = refused("While.apply")
    def copy(original: Tree)(cond: Term, body: Term): While = ReflectOps.copy(original, null, cond, body).asInstanceOf[While]
    def unapply(x: While): (Term, Term) = (x.cond, x.body)
  }

  object Try extends TryModule {
    def apply(expr: Term, cases: List[CaseDef], finalizer: Option[Term]): Try = refused("Try.apply")
    def copy(original: Tree)(expr: Term, cases: List[CaseDef], finalizer: Option[Term]): Try =
      ReflectOps.copy(original, null, expr, finalizer, cases).asInstanceOf[Try]
    def unapply(x: Try): (Term, List[CaseDef], Option[Term]) = (x.body, x.cases, x.finalizer)
  }

  object CaseDef extends CaseDefModule {
    def apply(pattern: Tree, guard: Option[Term], rhs: Term): CaseDef = {
      val args = Array(ReflectOps.id(pattern), guard.fold(-1)(ReflectOps.id), ReflectOps.id(rhs))
      ReflectOps.term(ReflectOps.one("caseDef", args*)).asInstanceOf[CaseDef]
    }
    def copy(original: Tree)(pattern: Tree, guard: Option[Term], rhs: Term): CaseDef =
      ReflectOps.copy(original, null, pattern, guard, rhs).asInstanceOf[CaseDef]
    def unapply(x: CaseDef): (Tree, Option[Term], Term) = (x.pattern, x.guard, x.rhs)
  }


  abstract class AssignMethodsImpl extends AssignMethods {
    extension (self: Assign) {
      def lhs: Term = ReflectOps.part(self, 0)
      def rhs: Term = ReflectOps.part(self, 1)
    }
  }

  abstract class NewMethodsImpl extends NewMethods {
    extension (self: New) def tpt: TypeTree = ReflectOps.part(self, 0)
  }

  abstract class SuperMethodsImpl extends SuperMethods {
    extension (self: Super) {
      def qualifier: Term = ReflectOps.part(self, 0)
      def id: Option[String] = Option(ReflectOps.shape(self).name)
      def idPos: Position = throw new Unsupported("Super.idPos")
    }
  }

  abstract class IfMethodsImpl extends IfMethods {
    extension (self: If) {
      def cond: Term = ReflectOps.part(self, 0)
      def thenp: Term = ReflectOps.part(self, 1)
      def elsep: Term = ReflectOps.part(self, 2)
      def isInline: Boolean = ReflectOps.shape(self).name != null
    }
  }

  abstract class MatchMethodsImpl extends MatchMethods {
    extension (self: Match) {
      def scrutinee: Term = ReflectOps.part(self, 0)
      def cases: List[CaseDef] = ReflectOps.shape(self).from(1).asInstanceOf[List[CaseDef]]
      def isInline: Boolean = ReflectOps.shape(self).name != null
    }
  }

  abstract class SummonFromMethodsImpl extends SummonFromMethods {
    extension (self: SummonFrom) def cases: List[CaseDef] = ReflectOps.shape(self).from(0).asInstanceOf[List[CaseDef]]
  }

  abstract class ReturnMethodsImpl extends ReturnMethods {
    extension (self: Return) {
      def expr: Term = ReflectOps.part(self, 0)
      def from: Symbol = ReflectOps.symbol(ReflectOps.part(self, 1)).asInstanceOf[Symbol]
    }
  }

  abstract class WhileMethodsImpl extends WhileMethods {
    extension (self: While) {
      def cond: Term = ReflectOps.part(self, 0)
      def body: Term = ReflectOps.part(self, 1)
    }
  }

  abstract class TryMethodsImpl extends TryMethods {
    extension (self: Try) {
      def body: Term = ReflectOps.part(self, 0)
      def cases: List[CaseDef] = ReflectOps.shape(self).from(2).asInstanceOf[List[CaseDef]]
      def finalizer: Option[Term] = ReflectOps.opt(ReflectOps.shape(self).children(1)).asInstanceOf[Option[Term]]
    }
  }

  abstract class CaseDefMethodsImpl extends CaseDefMethods {
    extension (self: CaseDef) {
      def pattern: Tree = ReflectOps.part(self, 0)
      def guard: Option[Term] = ReflectOps.opt(ReflectOps.shape(self).children(1)).asInstanceOf[Option[Term]]
      def rhs: Term = ReflectOps.part(self, 2)
    }
  }


  private def built(op: String, trees: Tree*): Any = ReflectOps.term(ReflectOps.one(op, trees.map(ReflectOps.id)*))

  private def test[T](kinds: Int*): T = new KindTest(kinds*).asInstanceOf[T]

  private def refused(what: String): Nothing = throw new Unsupported(what)

  private def methods[T](name: String): T =
    QuoteHost.allocate("scala.quoted.runtime.impl.ReflectControlSupport$ReflectControl$" + name).asInstanceOf[T]
}
}
