// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); changed:
//   compiler/src/scala/quoted/runtime/impl/QuotesImpl.scala
package scala.quoted.runtime.impl

import scala.reflect.TypeTest

/** The patterns of `quotes.reflect`. */
trait ReflectPatternSupport { this: QuotesImpl =>

trait ReflectPatterns extends reflectModule { self: reflect.type =>

  given BindTypeTest: TypeTest[Tree, Bind] = test(32)
  given UnapplyTypeTest: TypeTest[Tree, Unapply] = test(34)
  given AlternativesTypeTest: TypeTest[Tree, Alternatives] = test(35)
  given TypedOrTestTypeTest: TypeTest[Tree, TypedOrTest] = test(4, 36)
  given WildcardTypeTest: TypeTest[Tree, Wildcard] = new TypeTest[Tree, Wildcard] {
    def unapply(x: Tree): Option[x.type & Wildcard] =
      if (ReflectOps.kind(x) == 1 && ReflectOps.shape(x).name == "_") Some(x.asInstanceOf[x.type & Wildcard]) else None
  }

  given BindMethods: BindMethods = methods("BindMethodsImpl")
  given UnapplyMethods: UnapplyMethods = methods("UnapplyMethodsImpl")
  given AlternativesMethods: AlternativesMethods = methods("AlternativesMethodsImpl")
  given TypedOrTestMethods: TypedOrTestMethods = methods("TypedOrTestMethodsImpl")

  object Wildcard extends WildcardModule {
    def apply(): Wildcard = ReflectOps.term(ReflectOps.one("wildcard")).asInstanceOf[Wildcard]
    def unapply(pattern: Wildcard): true = true
  }

  object Bind extends BindModule {
    def apply(sym: Symbol, pattern: Tree): Bind = refused("Bind.apply")
    def copy(original: Tree)(name: String, pattern: Tree): Bind = ReflectOps.copy(original, name, pattern).asInstanceOf[Bind]
    def unapply(pattern: Bind): (String, Tree) = (pattern.name, pattern.pattern)
  }

  object Unapply extends UnapplyModule {
    def apply(fun: Term, implicits: List[Term], patterns: List[Tree]): Unapply = refused("Unapply.apply")
    def copy(original: Tree)(fun: Term, implicits: List[Term], patterns: List[Tree]): Unapply =
      ReflectOps.copy(original, implicits.length.toString, fun, implicits, patterns).asInstanceOf[Unapply]
    def unapply(x: Unapply): (Term, List[Term], List[Tree]) = (x.fun, x.implicits, x.patterns)
  }

  object Alternatives extends AlternativesModule {
    def apply(patterns: List[Tree]): Alternatives = refused("Alternatives.apply")
    def copy(original: Tree)(patterns: List[Tree]): Alternatives = ReflectOps.copy(original, null, patterns).asInstanceOf[Alternatives]
    def unapply(x: Alternatives): Some[List[Tree]] = Some(x.patterns)
  }

  object TypedOrTest extends TypedOrTestModule {
    def apply(expr: Tree, tpt: TypeTree): TypedOrTest = refused("TypedOrTest.apply")
    def copy(original: Tree)(expr: Tree, tpt: TypeTree): TypedOrTest =
      ReflectOps.copy(original, null, expr, tpt).asInstanceOf[TypedOrTest]
    def unapply(x: TypedOrTest): (Tree, TypeTree) = (x.tree, x.tpt)
  }

  abstract class BindMethodsImpl extends BindMethods {
    extension (self: Bind) {
      def name: String = ReflectOps.shape(self).name
      def pattern: Tree = ReflectOps.part(self, 0)
    }
  }

  /** The parts of an extractor pattern are its `unapply`, its implicit arguments and its
   *  patterns; a varargs extractor's are refused.
   */
  abstract class UnapplyMethodsImpl extends UnapplyMethods {
    extension (self: Unapply) {
      def fun: Term = ReflectOps.part(self, 0)
      def implicits: List[Term] = ReflectOps.from[Term](self, 1).take(ReflectOps.shape(self).name.toInt)
      def patterns: List[Tree] = {
        val patterns = ReflectOps.from[Tree](self, 1).drop(ReflectOps.shape(self).name.toInt)
        if (patterns.nonEmpty && ReflectOps.kind(patterns.last) == 9)
          throw new Unsupported("Unapply.patterns of a varargs extractor")
        patterns
      }
    }
  }

  abstract class AlternativesMethodsImpl extends AlternativesMethods {
    extension (self: Alternatives) def patterns: List[Tree] = ReflectOps.from(self, 0)
  }

  abstract class TypedOrTestMethodsImpl extends TypedOrTestMethods {
    extension (self: TypedOrTest) {
      def tree: Tree = ReflectOps.part(self, 0)
      def tpt: TypeTree = ReflectOps.part(self, 1)
    }
  }

  private def test[T](kinds: Int*): T = new KindTest(kinds*).asInstanceOf[T]

  private def refused(what: String): Nothing = throw new Unsupported(what)

  private def methods[T](name: String): T =
    QuoteHost.allocate("scala.quoted.runtime.impl.ReflectPatternSupport$ReflectPatterns$" + name).asInstanceOf[T]
}
}
