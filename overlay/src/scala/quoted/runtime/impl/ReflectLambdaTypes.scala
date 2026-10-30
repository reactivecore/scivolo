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
  given MethodTypeMethods: MethodTypeMethods = new MethodTypeMethodsImpl
  given PolyTypeMethods: PolyTypeMethods = methods("PolyTypeMethodsImpl")
  given TypeLambdaMethods: TypeLambdaMethods = methods("TypeLambdaMethodsImpl")
  given ParamRefMethods: ParamRefMethods = methods("ParamRefMethodsImpl")

  object MethodType extends MethodTypeModule {
    def apply(paramNames: List[String])(paramInfosExp: MethodType => List[TypeRepr], resultTypeExp: MethodType => TypeRepr): MethodType =
      make(0, paramNames, paramInfosExp, resultTypeExp)
    def apply(kind: MethodTypeKind)(paramNames: List[String])(paramInfosExp: MethodType => List[TypeRepr],
        resultTypeExp: MethodType => TypeRepr): MethodType =
      make(kind.ordinal, paramNames, paramInfosExp, resultTypeExp)
    def unapply(x: MethodType): (List[String], List[TypeRepr], TypeRepr) = (x.paramNames, x.paramTypes, x.resType)
  }

  object PolyType extends PolyTypeModule {
    def apply(paramNames: List[String])(paramBoundsExp: PolyType => List[TypeBounds], resultTypeExp: PolyType => TypeRepr): PolyType =
      make(3, paramNames, paramBoundsExp, resultTypeExp)
    def unapply(x: PolyType): (List[String], List[TypeBounds], TypeRepr) = (x.paramNames, x.paramBounds, x.resType)
  }

  object TypeLambda extends TypeLambdaModule {
    def apply(paramNames: List[String], boundsFn: TypeLambda => List[TypeBounds], bodyFn: TypeLambda => TypeRepr): TypeLambda =
      make(4, paramNames, boundsFn, bodyFn)
    def unapply(x: TypeLambda): (List[String], List[TypeBounds], TypeRepr) = (x.paramNames, x.paramBounds, x.resType)
  }

  /** A lambda type of kind `kind` (method types 0 plain, 1 implicit, 2 contextual; 3 polymorphic
   *  types, 4 type lambdas) whose parameter and result types `paramsExp` and `resultExp` give.
   */
  private def make[L, P](kind: Int, paramNames: List[String], paramsExp: L => List[P], resultExp: L => TypeRepr): L = {
    val binder = QuoteHost.tpe(ReflectOps.one("lambdaBinder", kind)).asInstanceOf[L]
    val types = paramsExp(binder) ::: List(resultExp(binder))
    ReflectOps.run("lambdaType", (ReflectOps.id(binder) :: types.map(ReflectOps.id)).toArray, paramNames.mkString("\u0000"))
    QuoteHost.tpe(QuoteHost.result(0)).asInstanceOf[L]
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

  /** Made with `new`, as `methodTypeKind` reads the enumeration of this `reflect`. */
  final class MethodTypeMethodsImpl extends MethodTypeMethods {
    extension (self: MethodType) {
      def isImplicit: Boolean = TypeShapes.lambda(self)._1(0) != 0
      def isContextual: Boolean = TypeShapes.lambda(self)._1(1) != 0
      def methodTypeKind: MethodTypeKind =
        if (self.isContextual) MethodTypeKind.Contextual
        else if (self.isImplicit) MethodTypeKind.Implicit
        else MethodTypeKind.Plain
      def isErased: Boolean = false
      def erasedParams: List[Boolean] = List.fill(TypeShapes.lambda(self)._1(2))(false)
      def hasErasedParams: Boolean = false
      def param(idx: Int): TypeRepr = LambdaTypes.paramRef(self, idx).asInstanceOf[TypeRepr]
    }
  }

  abstract class PolyTypeMethodsImpl extends PolyTypeMethods {
    extension (self: PolyType) {
      def param(idx: Int): TypeRepr = LambdaTypes.paramRef(self, idx).asInstanceOf[TypeRepr]
      def paramBounds: List[TypeBounds] = TypeShapes.lambda(self)._1.drop(3).init.map(QuoteHost.tpe).asInstanceOf[List[TypeBounds]]
    }
  }

  abstract class TypeLambdaMethodsImpl extends TypeLambdaMethods {
    extension (self: TypeLambda) {
      def param(idx: Int): TypeRepr = LambdaTypes.paramRef(self, idx).asInstanceOf[TypeRepr]
      def paramBounds: List[TypeBounds] = TypeShapes.lambda(self)._1.drop(3).init.map(QuoteHost.tpe).asInstanceOf[List[TypeBounds]]
      def paramVariances: List[Flags] = ReflectOps.results("paramVariances", ReflectOps.id(self))
        .map(v => new FlagsImpl((v & 1L) << 28 | (v & 2L) << 28).asInstanceOf[Flags])
    }
  }

  abstract class ParamRefMethodsImpl extends ParamRefMethods {
    extension (self: ParamRef) {
      def binder: TypeRepr = throw new Unsupported("ParamRef.binder")
      def paramNum: Int = ReflectOps.one("paramNum", ReflectOps.id(self))
    }
  }

  private def test[T](kinds: Int*): T = new TypeKindTest(kinds*).asInstanceOf[T]

  private def methods[T](name: String): T =
    QuoteHost.allocate("scala.quoted.runtime.impl.ReflectLambdaTypeSupport$ReflectLambdaTypes$" + name).asInstanceOf[T]
}
}

object LambdaTypes {
  /** The reference to parameter `idx` of lambda type `lambda`. */
  def paramRef(lambda: Any, idx: Int): Any = QuoteHost.tpe(ReflectOps.one("paramRef", ReflectOps.id(lambda), idx))
}
