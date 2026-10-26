// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); changed:
//   compiler/src/scala/quoted/runtime/impl/QuotesImpl.scala
package scala.quoted.runtime.impl

import scala.reflect.TypeTest

/** The named, singleton, refined, applied and annotated type trees of `quotes.reflect`. */
trait ReflectTypeTreeSupport { this: QuotesImpl =>

trait ReflectTypeTrees extends reflectModule { self: reflect.type =>

  given TypeIdentTypeTest: TypeTest[Tree, TypeIdent] = test(37)
  given TypeSelectTypeTest: TypeTest[Tree, TypeSelect] = test(38)
  given TypeProjectionTypeTest: TypeTest[Tree, TypeProjection] = test(39)
  given SingletonTypeTest: TypeTest[Tree, Singleton] = test(40)
  given RefinedTypeTest: TypeTest[Tree, Refined] = test(41)
  given AppliedTypeTest: TypeTest[Tree, Applied] = test(42)
  given AnnotatedTypeTest: TypeTest[Tree, Annotated] = test(43)

  given TypeIdentMethods: TypeIdentMethods = methods("TypeIdentMethodsImpl")
  given TypeSelectMethods: TypeSelectMethods = methods("TypeSelectMethodsImpl")
  given TypeProjectionMethods: TypeProjectionMethods = methods("TypeProjectionMethodsImpl")
  given SingletonMethods: SingletonMethods = methods("SingletonMethodsImpl")
  given RefinedMethods: RefinedMethods = methods("RefinedMethodsImpl")
  given AppliedMethods: AppliedMethods = methods("AppliedMethodsImpl")
  given AnnotatedMethods: AnnotatedMethods = methods("AnnotatedMethodsImpl")

  object TypeIdent extends TypeIdentModule {
    def apply(sym: Symbol): TypeTree = ReflectOps.term(ReflectOps.one("typeIdent", ReflectOps.id(sym))).asInstanceOf[TypeTree]
    def copy(original: Tree)(name: String): TypeIdent = copied(original, name)
    def unapply(x: TypeIdent): Some[String] = Some(x.name)
  }

  object TypeSelect extends TypeSelectModule {
    def apply(qualifier: Term, name: String): TypeSelect = refused("TypeSelect.apply")
    def copy(original: Tree)(qualifier: Term, name: String): TypeSelect = copied(original, name, qualifier)
    def unapply(x: TypeSelect): (Term, String) = (x.qualifier, x.name)
  }

  object TypeProjection extends TypeProjectionModule {
    def apply(qualifier: TypeTree, name: String): TypeProjection = refused("TypeProjection.apply")
    def copy(original: Tree)(qualifier: TypeTree, name: String): TypeProjection = copied(original, name, qualifier)
    def unapply(x: TypeProjection): (TypeTree, String) = (x.qualifier, x.name)
  }

  object Singleton extends SingletonModule {
    def apply(ref: Term): Singleton = refused("Singleton.apply")
    def copy(original: Tree)(ref: Term): Singleton = copied(original, null, ref)
    def unapply(x: Singleton): Some[Term] = Some(x.ref)
  }

  object Refined extends RefinedModule {
    def apply(tpt: TypeTree, refinements: List[Definition], refineCls: Symbol): Refined = refused("Refined.apply")
    def copy(original: Tree)(tpt: TypeTree, refinements: List[Definition]): Refined = copied(original, null, tpt, refinements)
    def unapply(x: Refined): (TypeTree, List[Definition]) = (x.tpt, x.refinements)
  }

  object Applied extends AppliedModule {
    def apply(tpt: TypeTree, args: List[Tree]): Applied = refused("Applied.apply")
    def copy(original: Tree)(tpt: TypeTree, args: List[Tree]): Applied = copied(original, null, tpt, args)
    def unapply(x: Applied): (TypeTree, List[Tree]) = (x.tpt, x.args)
  }

  object Annotated extends AnnotatedModule {
    def apply(arg: TypeTree, annotation: Term): Annotated = refused("Annotated.apply")
    def copy(original: Tree)(arg: TypeTree, annotation: Term): Annotated = copied(original, null, arg, annotation)
    def unapply(x: Annotated): (TypeTree, Term) = (x.arg, x.annotation)
  }

  abstract class TypeIdentMethodsImpl extends TypeIdentMethods {
    extension (self: TypeIdent) def name: String = ReflectOps.shape(self).name
  }

  abstract class TypeSelectMethodsImpl extends TypeSelectMethods {
    extension (self: TypeSelect) {
      def qualifier: Term = ReflectOps.part(self, 0)
      def name: String = ReflectOps.shape(self).name
    }
  }

  abstract class TypeProjectionMethodsImpl extends TypeProjectionMethods {
    extension (self: TypeProjection) {
      def qualifier: TypeTree = ReflectOps.part(self, 0)
      def name: String = ReflectOps.shape(self).name
    }
  }

  abstract class SingletonMethodsImpl extends SingletonMethods {
    extension (self: Singleton) def ref: Term = ReflectOps.part(self, 0)
  }

  abstract class RefinedMethodsImpl extends RefinedMethods {
    extension (self: Refined) {
      def tpt: TypeTree = ReflectOps.part(self, 0)
      def refinements: List[Definition] = ReflectOps.from(self, 1)
    }
  }

  abstract class AppliedMethodsImpl extends AppliedMethods {
    extension (self: Applied) {
      def tpt: TypeTree = ReflectOps.part(self, 0)
      def args: List[Tree] = ReflectOps.from(self, 1)
    }
  }

  abstract class AnnotatedMethodsImpl extends AnnotatedMethods {
    extension (self: Annotated) {
      def arg: TypeTree = ReflectOps.part(self, 0)
      def annotation: Term = ReflectOps.part(self, 1)
    }
  }

  private def copied[T](original: Tree, name: String, parts: Any*): T = ReflectOps.copy(original, name, parts*).asInstanceOf[T]

  private def test[T](kinds: Int*): T = new KindTest(kinds*).asInstanceOf[T]

  private def refused(what: String): Nothing = throw new Unsupported(what)

  private def methods[T](name: String): T =
    QuoteHost.allocate("scala.quoted.runtime.impl.ReflectTypeTreeSupport$ReflectTypeTrees$" + name).asInstanceOf[T]
}
}
