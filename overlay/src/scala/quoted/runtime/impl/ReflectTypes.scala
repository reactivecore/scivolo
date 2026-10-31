// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); changed:
//   compiler/src/scala/quoted/runtime/impl/QuotesImpl.scala
package scala.quoted.runtime.impl

import scala.quoted.Type
import scala.reflect.TypeTest

/** The types of `quotes.reflect`. */
trait ReflectTypeSupport { this: QuotesImpl =>

trait ReflectTypes extends reflectModule { self: reflect.type =>

  given TypeReprMethods: TypeReprMethods = methods("TypeReprMethodsImpl")

  object TypeRepr extends TypeReprModule {
    def of[T <: AnyKind](using t: Type[T]): TypeRepr = QuoteHost.tpe(QuoteHost.typeId(t)).asInstanceOf[TypeRepr]
    def typeConstructorOf(clazz: Class[?]): TypeRepr = {
      ReflectOps.run("typeConstructorOf", new Array[Int](0), clazz.getName)
      QuoteHost.tpe(QuoteHost.result(0)).asInstanceOf[TypeRepr]
    }
  }

  given ConstantTypeTypeTest: TypeTest[TypeRepr, ConstantType] =
    (new ConstantTypeTest).asInstanceOf[TypeTest[TypeRepr, ConstantType]]
  given ConstantTypeMethods: ConstantTypeMethods = methods("ConstantTypeMethodsImpl")

  object ConstantType extends ConstantTypeModule {
    def apply(x: Constant): ConstantType = {
      val c = x.asInstanceOf[ConstantImpl]
      ReflectOps.run("constantTypeOf", Array(c.kind, c.lo, c.hi), c.text)
      QuoteHost.tpe(QuoteHost.result(0)).asInstanceOf[ConstantType]
    }
    def unapply(x: ConstantType): Some[Constant] = Some(x.constant)
  }

  abstract class ConstantTypeMethodsImpl extends ConstantTypeMethods {
    extension (self: ConstantType) def constant: Constant =
      ConstantTypeTest.constant(self).getOrElse(throw new Unsupported("a constant type")).asInstanceOf[Constant]
  }

  abstract class TypeReprMethodsImpl extends TypeReprMethods {
    private def repr(id: Int): TypeRepr = QuoteHost.tpe(id).asInstanceOf[TypeRepr]

    extension (self: TypeRepr) {
      def =:=(that: TypeRepr): Boolean = ReflectOps.one("isSameType", ReflectOps.id(self), ReflectOps.id(that)) != 0
      def <:<(that: TypeRepr): Boolean = ReflectOps.one("isSubtype", ReflectOps.id(self), ReflectOps.id(that)) != 0
      def asType: Type[?] = QuoteHost.quotedType(ReflectOps.id(self))
      def show(using printer: Printer[TypeRepr]): String = printer.show(self)
      def dealias: TypeRepr = repr(ReflectOps.one("dealias", ReflectOps.id(self)))
      def isSingleton: Boolean = ReflectOps.one("isSingleton", ReflectOps.id(self)) != 0
      def baseClasses: List[Symbol] =
        ReflectOps.results("baseClasses", ReflectOps.id(self)).map(s => new SymbolImpl(s).asInstanceOf[Symbol])
      def substituteTypes(from: List[Symbol], to: List[TypeRepr]): TypeRepr =
        val pairs = from.zip(to).flatMap((s, t) => List(ReflectOps.id(s), ReflectOps.id(t)))
        repr(ReflectOps.one("substituteTypes", ReflectOps.id(self) :: pairs*))
      def memberType(member: Symbol): TypeRepr =repr(ReflectOps.one("memberType", ReflectOps.id(self), ReflectOps.id(member)))
      def appliedTo(targ: TypeRepr): TypeRepr = TypeShapes.appliedTo(self, List(targ)).asInstanceOf[TypeRepr]
      def appliedTo(targs: List[TypeRepr]): TypeRepr = TypeShapes.appliedTo(self, targs).asInstanceOf[TypeRepr]
      def derivesFrom(cls: Symbol): Boolean = baseClasses.contains(cls)
      def classSymbol: Option[Symbol] = {
        val c = ReflectOps.one("classSymbol", ReflectOps.id(self))
        if (c < 0) None else Some(new SymbolImpl(c).asInstanceOf[Symbol])
      }
      def widen: TypeRepr = QuoteHost.tpe(ReflectOps.one("widen", ReflectOps.id(self))).asInstanceOf[TypeRepr]
      def widenTermRefByName: TypeRepr =
        QuoteHost.tpe(ReflectOps.one("widenTermRefByName", ReflectOps.id(self))).asInstanceOf[TypeRepr]
      def typeSymbol: Symbol = new SymbolImpl(ReflectOps.one("typeSymbol", ReflectOps.id(self))).asInstanceOf[Symbol]
      def termSymbol: Symbol = new SymbolImpl(ReflectOps.one("termSymbol", ReflectOps.id(self))).asInstanceOf[Symbol]
      def typeArgs: List[TypeRepr] =
        ReflectOps.results("typeArgs", ReflectOps.id(self)).map(t => QuoteHost.tpe(t).asInstanceOf[TypeRepr])
      def baseType(cls: Symbol): TypeRepr = repr(ReflectOps.one("baseType", ReflectOps.id(self), ReflectOps.id(cls)))
      def select(sym: Symbol): TypeRepr = repr(ReflectOps.one("selectType", ReflectOps.id(self), ReflectOps.id(sym)))
      def widenByName: TypeRepr = if (TypeShapes.kind(self) == 14) TypeShapes.part(self, 0).asInstanceOf[TypeRepr] else self
      def dealiasKeepOpaques: TypeRepr = repr(ReflectOps.one("dealiasKeepOpaques", ReflectOps.id(self)))
      def simplified: TypeRepr = throw new Unsupported("TypeRepr.simplified")
      def isTupleN: Boolean = applied(self, "scala.Tuple", 0)
      def isFunctionType: Boolean = {
        val d = self.dealias
        function(if (refinedFunction(d)) TypeShapes.part(d, 0).asInstanceOf[TypeRepr] else d)
      }
      def isContextFunctionType: Boolean = applied(self, "scala.ContextFunction", 1)
      def isErasedFunctionType: Boolean = ReflectOps.one("isErasedFunctionType", ReflectOps.id(self)) != 0
      def isDependentFunctionType: Boolean = refinedFunction(self.dealias)
    }

    private def function(t: TypeRepr): Boolean = applied(t, "scala.Function", 1) || applied(t, "scala.ContextFunction", 1)

    /** Whether `d` refines method `apply` of a function type (dotty's `dropDependentRefinement`). */
    private def refinedFunction(d: TypeRepr): Boolean =
      TypeShapes.kind(d) == 6 && TypeShapes.name(d) == "apply" && function(TypeShapes.part(d, 0).asInstanceOf[TypeRepr])

    /** Whether `t` dealiased is class `prefix` + n applied to n + `extra` arguments. */
    private def applied(t: TypeRepr, prefix: String, extra: Int): Boolean = {
      val d = t.dealias
      val n = d.typeArgs.length - extra
      val sym = ReflectOps.id(d.typeSymbol)
      n >= 0 && sym >= 0 && ReflectOps.text("fullName", sym) == prefix + n
    }
  }

  private def methods[T](name: String): T =
    QuoteHost.allocate("scala.quoted.runtime.impl.ReflectTypeSupport$ReflectTypes$" + name).asInstanceOf[T]
}
}

/** The type test of constant types. */
final class ConstantTypeTest extends TypeTest[Any, Any] {
  def unapply(x: Any): Option[x.type & Any] = ConstantTypeTest.constant(x).map(_ => x)
}

object ConstantTypeTest {
  /** The constant of type `t`, if it is a constant type. */
  def constant(t: Any): Option[ConstantImpl] = ReflectOps.results("constantType", ReflectOps.id(t)) match {
    case List(kind, lo, hi) => Some(new ConstantImpl(kind, lo, hi, QuoteHost.resultText()))
    case _                  => None
  }
}
