// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); changed:
//   compiler/src/scala/quoted/runtime/impl/QuotesImpl.scala
package scala.quoted.runtime.impl

import scala.reflect.TypeTest

/** The method types, polymorphic types, type lambdas and parameter references of
 *  `quotes.reflect`.
 */
trait ReflectLambdaTypeSupport { this: QuotesImpl =>

trait ReflectLambdaTypes extends reflectModule { self: reflect.type =>

  given LambdaTypeTypeTest: TypeTest[TypeRepr, LambdaType] = test(8, 9, 11)
  given MethodOrPolyTypeTest: TypeTest[TypeRepr, MethodOrPoly] = test(8, 9)
  given MethodTypeTypeTest: TypeTest[TypeRepr, MethodType] = test(8)
  given PolyTypeTypeTest: TypeTest[TypeRepr, PolyType] = test(9)
  given TypeLambdaTypeTest: TypeTest[TypeRepr, TypeLambda] = test(11)
  given ParamRefTypeTest: TypeTest[TypeRepr, ParamRef] = test(15)

  given LambdaTypeMethods: LambdaTypeMethods = methods("LambdaTypeMethodsImpl")
  given MethodTypeMethods: MethodTypeMethods = methods("MethodTypeMethodsImpl")
  given PolyTypeMethods: PolyTypeMethods = methods("PolyTypeMethodsImpl")
  given TypeLambdaMethods: TypeLambdaMethods = methods("TypeLambdaMethodsImpl")
  given ParamRefMethods: ParamRefMethods = methods("ParamRefMethodsImpl")

  /** A method type of parameter and result types that do not refer to it. */
  object MethodType extends MethodTypeModule {
    def apply(paramNames: List[String])(paramInfosExp: MethodType => List[TypeRepr], resultTypeExp: MethodType => TypeRepr): MethodType =
      make(0, paramNames, paramInfosExp, resultTypeExp)
    def apply(kind: MethodTypeKind)(paramNames: List[String])(paramInfosExp: MethodType => List[TypeRepr],
        resultTypeExp: MethodType => TypeRepr): MethodType =
      make(kind.ordinal, paramNames, paramInfosExp, resultTypeExp)
    def unapply(x: MethodType): (List[String], List[TypeRepr], TypeRepr) = (x.paramNames, x.paramTypes, x.resType)

    /** A method type of kind `kind`: 0 plain, 1 implicit, 2 contextual. */
    private def make(kind: Int, paramNames: List[String], paramInfosExp: MethodType => List[TypeRepr],
        resultTypeExp: MethodType => TypeRepr): MethodType = {
      val unknown = QuoteHost.tpe(Int.MaxValue).asInstanceOf[MethodType]
      val types = paramInfosExp(unknown) ::: List(resultTypeExp(unknown))
      ReflectOps.run("methodType", (kind :: types.map(ReflectOps.id)).toArray, paramNames.mkString("\u0000"))
      QuoteHost.tpe(QuoteHost.result(0)).asInstanceOf[MethodType]
    }
  }

  object PolyType extends PolyTypeModule {
    def apply(paramNames: List[String])(paramBoundsExp: PolyType => List[TypeBounds], resultTypeExp: PolyType => TypeRepr): PolyType =
      refused("PolyType.apply")
    def unapply(x: PolyType): (List[String], List[TypeBounds], TypeRepr) = (x.paramNames, x.paramBounds, x.resType)
  }

  object TypeLambda extends TypeLambdaModule {
    def apply(paramNames: List[String], boundsFn: TypeLambda => List[TypeBounds], bodyFn: TypeLambda => TypeRepr): TypeLambda =
      refused("TypeLambda.apply")
    def unapply(x: TypeLambda): (List[String], List[TypeBounds], TypeRepr) = (x.paramNames, x.paramBounds, x.resType)
  }

  object ParamRef extends ParamRefModule {
    def unapply(x: ParamRef): (TypeRepr, Int) = (x.binder, x.paramNum)
  }

  /** Parts 0 to 2 of a method, polymorphic type or type lambda: whether it is implicit,
   *  contextual, how many parameters it has; then their types, its result type and the names of
   *  its parameters.
   */
  abstract class LambdaTypeMethodsImpl extends LambdaTypeMethods {
    extension (self: LambdaType) {
      def paramNames: List[String] = TypeShapes.lambda(self)._2
      def paramTypes: List[TypeRepr] = TypeShapes.lambda(self)._1.drop(3).init.map(QuoteHost.tpe).asInstanceOf[List[TypeRepr]]
      def resType: TypeRepr = QuoteHost.tpe(TypeShapes.lambda(self)._1.last).asInstanceOf[TypeRepr]
    }
  }

  abstract class MethodTypeMethodsImpl extends MethodTypeMethods {
    extension (self: MethodType) {
      def isImplicit: Boolean = TypeShapes.lambda(self)._1(0) != 0
      def isContextual: Boolean = TypeShapes.lambda(self)._1(1) != 0
      def methodTypeKind: MethodTypeKind = throw new Unsupported("MethodType.methodTypeKind")
      def isErased: Boolean = false
      def erasedParams: List[Boolean] = List.fill(TypeShapes.lambda(self)._1(2))(false)
      def hasErasedParams: Boolean = false
      def param(idx: Int): TypeRepr = throw new Unsupported("MethodType.param")
    }
  }

  abstract class PolyTypeMethodsImpl extends PolyTypeMethods {
    extension (self: PolyType) {
      def param(idx: Int): TypeRepr = throw new Unsupported("PolyType.param")
      def paramBounds: List[TypeBounds] = TypeShapes.lambda(self)._1.drop(3).init.map(QuoteHost.tpe).asInstanceOf[List[TypeBounds]]
    }
  }

  abstract class TypeLambdaMethodsImpl extends TypeLambdaMethods {
    extension (self: TypeLambda) {
      def param(idx: Int): TypeRepr = throw new Unsupported("TypeLambda.param")
      def paramBounds: List[TypeBounds] = TypeShapes.lambda(self)._1.drop(3).init.map(QuoteHost.tpe).asInstanceOf[List[TypeBounds]]
      def paramVariances: List[Flags] = throw new Unsupported("TypeLambda.paramVariances")
    }
  }

  abstract class ParamRefMethodsImpl extends ParamRefMethods {
    extension (self: ParamRef) {
      def binder: TypeRepr = throw new Unsupported("ParamRef.binder")
      def paramNum: Int = ReflectOps.one("paramNum", ReflectOps.id(self))
    }
  }

  private def test[T](kinds: Int*): T = new TypeKindTest(kinds*).asInstanceOf[T]

  private def refused(what: String): Nothing = throw new Unsupported(what)

  private def methods[T](name: String): T =
    QuoteHost.allocate("scala.quoted.runtime.impl.ReflectLambdaTypeSupport$ReflectLambdaTypes$" + name).asInstanceOf[T]
}
}
