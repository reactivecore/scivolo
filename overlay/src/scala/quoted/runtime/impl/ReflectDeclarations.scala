// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); changed:
//   compiler/src/scala/quoted/runtime/impl/QuotesImpl.scala
package scala.quoted.runtime.impl

/** The declarations and type members of classes in `quotes.reflect`. */
trait ReflectDeclarationSupport { this: QuotesImpl =>

trait ReflectDeclarations extends reflectModule { self: reflect.type =>

  trait SymbolDeclarations extends SymbolMethods {
    extension (self: Symbol) {
      def declaredMethods: List[Symbol] = declared(self, "methods", "")
      def declaredFields: List[Symbol] = declared(self, "fields", "")
      def declaredField(name: String): Symbol = first(declared(self, "fields", name))
      def declaredTypes: List[Symbol] = declared(self, "types", "")
      def declaredType(name: String): List[Symbol] = declared(self, "types", name)
      def memberTypes: List[Symbol] = declared(self, "types", "")
      def memberType(name: String): Symbol = first(declared(self, "types", name))
      def declarations: List[Symbol] = declared(self, "all", "")
      def typeMember(name: String): Symbol = {
        ReflectOps.run("typeMember", Array(ReflectOps.id(self)), name)
        new SymbolImpl(QuoteHost.result(0)).asInstanceOf[Symbol]
      }
    }

    private def first(syms: List[Symbol]): Symbol = syms.headOption.getOrElse(new SymbolImpl(-1).asInstanceOf[Symbol])

    /** The declarations of `s` of kind `what` named `name`, of any name if it is empty. */
    private def declared(s: Symbol, what: String, name: String): List[Symbol] = {
      val n = ReflectOps.run("declared", Array(ReflectOps.id(s)), what + "\u0000" + name)
      List.tabulate(n)(i => new SymbolImpl(QuoteHost.result(i)).asInstanceOf[Symbol])
    }
  }
}
}
