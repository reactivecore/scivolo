// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); changed:
//   compiler/src/scala/quoted/runtime/impl/QuotesImpl.scala
package scala.quoted.runtime.impl

import scala.quoted.Type

/** The types of `quotes.reflect`. */
trait ReflectTypeSupport { this: QuotesImpl =>

trait ReflectTypes extends reflectModule { self: reflect.type =>

  given TypeReprMethods: TypeReprMethods = methods("TypeReprMethodsImpl")

  object TypeRepr extends TypeReprModule {
    def of[T <: AnyKind](using t: Type[T]): TypeRepr = QuoteHost.tpe(QuoteHost.typeId(t)).asInstanceOf[TypeRepr]
    def typeConstructorOf(clazz: Class[?]): TypeRepr = throw new Unsupported("TypeRepr.typeConstructorOf")
  }

  abstract class TypeReprMethodsImpl extends TypeReprMethods {
    extension (self: TypeRepr) {
      def typeSymbol: Symbol = new SymbolImpl(ReflectOps.one("typeSymbol", ReflectOps.id(self))).asInstanceOf[Symbol]
      def typeArgs: List[TypeRepr] =
        ReflectOps.results("typeArgs", ReflectOps.id(self)).map(t => QuoteHost.tpe(t).asInstanceOf[TypeRepr])
    }
  }

  private def methods[T](name: String): T =
    QuoteHost.allocate("scala.quoted.runtime.impl.ReflectTypeSupport$ReflectTypes$" + name).asInstanceOf[T]
}
}
