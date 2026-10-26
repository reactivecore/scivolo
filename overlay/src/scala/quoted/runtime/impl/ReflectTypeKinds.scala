// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); changed:
//   compiler/src/scala/quoted/runtime/impl/QuotesImpl.scala
package scala.quoted.runtime.impl

import scala.reflect.TypeTest

/** The `this`, `super`, union, intersection, by-name, match and recursive types of
 *  `quotes.reflect`.
 */
trait ReflectTypeKindSupport { this: QuotesImpl =>

trait ReflectTypeKinds extends reflectModule { self: reflect.type =>

  given ThisTypeTypeTest: TypeTest[TypeRepr, ThisType] = test(10)
  given AndOrTypeTypeTest: TypeTest[TypeRepr, AndOrType] = test(12, 13)
  given AndTypeTypeTest: TypeTest[TypeRepr, AndType] = test(12)
  given OrTypeTypeTest: TypeTest[TypeRepr, OrType] = test(13)
  given ByNameTypeTypeTest: TypeTest[TypeRepr, ByNameType] = test(14)
  given SuperTypeTypeTest: TypeTest[TypeRepr, SuperType] = test(16)
  given MatchTypeTypeTest: TypeTest[TypeRepr, MatchType] = test(17)
  given RecursiveTypeTypeTest: TypeTest[TypeRepr, RecursiveType] = test(18)
  given RecursiveThisTypeTest: TypeTest[TypeRepr, RecursiveThis] = test(19)
  given MatchCaseTypeTest: TypeTest[TypeRepr, MatchCase] = test(20)

  given ThisTypeMethods: ThisTypeMethods = methods("ThisTypeMethodsImpl")
  given AndOrTypeMethods: AndOrTypeMethods = methods("AndOrTypeMethodsImpl")
  given ByNameTypeMethods: ByNameTypeMethods = methods("ByNameTypeMethodsImpl")
  given SuperTypeMethods: SuperTypeMethods = methods("SuperTypeMethodsImpl")
  given MatchTypeMethods: MatchTypeMethods = methods("MatchTypeMethodsImpl")
  given RecursiveTypeMethods: RecursiveTypeMethods = methods("RecursiveTypeMethodsImpl")
  given RecursiveThisMethods: RecursiveThisMethods = methods("RecursiveThisMethodsImpl")
  given MatchCaseMethods: MatchCaseMethods = methods("MatchCaseMethodsImpl")

  object ThisType extends ThisTypeModule {
    def unapply(x: ThisType): Some[TypeRepr] = Some(x.tref)
  }

  object AndType extends AndTypeModule {
    def apply(lhs: TypeRepr, rhs: TypeRepr): AndType = refused("AndType.apply")
    def unapply(x: AndType): (TypeRepr, TypeRepr) = (x.left, x.right)
  }

  object OrType extends OrTypeModule {
    def apply(lhs: TypeRepr, rhs: TypeRepr): OrType = refused("OrType.apply")
    def unapply(x: OrType): (TypeRepr, TypeRepr) = (x.left, x.right)
  }

  object ByNameType extends ByNameTypeModule {
    def apply(underlying: TypeRepr): TypeRepr =
      QuoteHost.tpe(ReflectOps.one("byName", ReflectOps.id(underlying))).asInstanceOf[TypeRepr]
    def unapply(x: ByNameType): Some[TypeRepr] = Some(x.underlying)
  }

  object SuperType extends SuperTypeModule {
    def apply(thistpe: TypeRepr, supertpe: TypeRepr): SuperType = refused("SuperType.apply")
    def unapply(x: SuperType): (TypeRepr, TypeRepr) = (x.thistpe, x.supertpe)
  }

  object MatchType extends MatchTypeModule {
    def apply(bound: TypeRepr, scrutinee: TypeRepr, cases: List[TypeRepr]): MatchType = refused("MatchType.apply")
    def unapply(x: MatchType): (TypeRepr, TypeRepr, List[TypeRepr]) = (x.bound, x.scrutinee, x.cases)
  }

  object RecursiveType extends RecursiveTypeModule {
    def apply(parentExp: RecursiveType => TypeRepr): RecursiveType = refused("RecursiveType.apply")
    def unapply(x: RecursiveType): Some[TypeRepr] = Some(x.underlying)
  }

  object RecursiveThis extends RecursiveThisModule {
    def unapply(x: RecursiveThis): Some[RecursiveType] = Some(x.binder)
  }

  object MatchCase extends MatchCaseModule {
    def apply(pattern: TypeRepr, rhs: TypeRepr): MatchCase = refused("MatchCase.apply")
    def unapply(x: MatchCase): (TypeRepr, TypeRepr) = (x.pattern, x.rhs)
  }

  abstract class ThisTypeMethodsImpl extends ThisTypeMethods {
    extension (self: ThisType) def tref: TypeRepr = TypeShapes.part(self, 0).asInstanceOf[TypeRepr]
  }

  abstract class AndOrTypeMethodsImpl extends AndOrTypeMethods {
    extension (self: AndOrType) {
      def left: TypeRepr = TypeShapes.part(self, 0).asInstanceOf[TypeRepr]
      def right: TypeRepr = TypeShapes.part(self, 1).asInstanceOf[TypeRepr]
    }
  }

  abstract class ByNameTypeMethodsImpl extends ByNameTypeMethods {
    extension (self: ByNameType) def underlying: TypeRepr = TypeShapes.part(self, 0).asInstanceOf[TypeRepr]
  }

  abstract class SuperTypeMethodsImpl extends SuperTypeMethods {
    extension (self: SuperType) {
      def thistpe: TypeRepr = TypeShapes.part(self, 0).asInstanceOf[TypeRepr]
      def supertpe: TypeRepr = TypeShapes.part(self, 1).asInstanceOf[TypeRepr]
    }
  }

  abstract class MatchTypeMethodsImpl extends MatchTypeMethods {
    extension (self: MatchType) {
      def bound: TypeRepr = TypeShapes.part(self, 0).asInstanceOf[TypeRepr]
      def scrutinee: TypeRepr = TypeShapes.part(self, 1).asInstanceOf[TypeRepr]
      def cases: List[TypeRepr] = TypeShapes.parts(self).drop(2).asInstanceOf[List[TypeRepr]]
    }
  }

  abstract class RecursiveTypeMethodsImpl extends RecursiveTypeMethods {
    extension (self: RecursiveType) {
      def underlying: TypeRepr = TypeShapes.part(self, 0).asInstanceOf[TypeRepr]
      def recThis: RecursiveThis = throw new Unsupported("RecursiveType.recThis")
    }
  }

  abstract class RecursiveThisMethodsImpl extends RecursiveThisMethods {
    extension (self: RecursiveThis) def binder: RecursiveType = throw new Unsupported("RecursiveThis.binder")
  }

  abstract class MatchCaseMethodsImpl extends MatchCaseMethods {
    extension (self: MatchCase) {
      def pattern: TypeRepr = TypeShapes.part(self, 0).asInstanceOf[TypeRepr]
      def rhs: TypeRepr = TypeShapes.part(self, 1).asInstanceOf[TypeRepr]
    }
  }

  private def test[T](kinds: Int*): T = new TypeKindTest(kinds*).asInstanceOf[T]

  private def refused(what: String): Nothing = throw new Unsupported(what)

  private def methods[T](name: String): T =
    QuoteHost.allocate("scala.quoted.runtime.impl.ReflectTypeKindSupport$ReflectTypeKinds$" + name).asInstanceOf[T]
}
}
