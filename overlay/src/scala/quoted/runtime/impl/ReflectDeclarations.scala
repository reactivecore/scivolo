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
      def memberType(name: String): Symbol = self.typeMember(name)
      def declarations: List[Symbol] = declared(self, "all", "")
      def typeMember(name: String): Symbol = {
        ReflectOps.run("typeMember", Array(ReflectOps.id(self)), name)
        new SymbolImpl(QuoteHost.result(0)).asInstanceOf[Symbol]
      }
      def typeMembers: List[Symbol] = members("typeMembers", self, "")
      def fieldMembers: List[Symbol] = members("fieldMembers", self, "")
      def memberFields: List[Symbol] = self.fieldMembers
      def memberField(name: String): Symbol = first(members("fieldMembers", self, name))
      def isTypeParam: Boolean = kind(self) == 3
      def isAliasType: Boolean = ReflectOps.id(self) >= 0 && ReflectOps.one("isAliasType", ReflectOps.id(self)) != 0
      def isAbstractType: Boolean = (kind(self) == 2 || kind(self) == 3) && !self.isAliasType
      def isAnonymousClass: Boolean = kind(self) == 1 && self.name.startsWith("$anon")
      def isAnonymousFunction: Boolean = kind(self) == 6 && self.name.startsWith("$anonfun")
      def isRefinementClass: Boolean = kind(self) == 1 && self.name == "<refinement>"
      def isSuperAccessor: Boolean = kind(self) == 6 && self.name.startsWith("super$")
      def isBind: Boolean = kind(self) >= 4 && (bits(self) & (1L << 12 | 1L << 19 | 1L << 21)) == 1L << 12
      def maybeOwner: Symbol = if (ReflectOps.id(self) < 0) self else new SymbolImpl(ReflectOps.one("owner", ReflectOps.id(self))).asInstanceOf[Symbol]
      def paramVariance: Flags = new FlagsImpl(bits(self) & (1L << 28 | 1L << 29)).asInstanceOf[Flags]
      def privateWithin: Option[TypeRepr] = within(self, "Symbol.privateWithin")
      def protectedWithin: Option[TypeRepr] = within(self, "Symbol.protectedWithin")
      def docstring: Option[String] = throw new Unsupported("Symbol.docstring")
      def signature: Signature = {
        val numbers = ReflectOps.results("signature", ReflectOps.id(self))
        val names = QuoteHost.resultText().split("\u0000", -1).toList
        val (params, _) = numbers.foldLeft((List.empty[String | Int], names)) {
          case ((out, rest), -1) => (rest.head :: out, rest.tail)
          case ((out, rest), n)  => (n :: out, rest)
        }
        new SignatureImpl(params.reverse, names.last).asInstanceOf[Signature]
      }
      def allOverriddenSymbols: Iterator[Symbol] = throw new Unsupported("Symbol.allOverriddenSymbols")
      def isDefinedInCurrentRun: Boolean = throw new Unsupported("Symbol.isDefinedInCurrentRun")
    }

    private def kind(s: Symbol): Int = if (ReflectOps.id(s) < 0) -1 else ReflectOps.one("kind", ReflectOps.id(s))

    private def bits(s: Symbol): Long = if (ReflectOps.id(s) < 0) 0L else ReflectOps.flags(ReflectOps.id(s)).bits

    /** `None` for a symbol neither private nor protected; the qualifier of one that is, which the
     *  frontend does not keep, is refused.
     */
    private def within(s: Symbol, what: String): Option[TypeRepr] =
      if ((bits(s) & (1L << 6 | 1L << 8)) == 0) None else throw new Unsupported(what)

    private def members(op: String, s: Symbol, name: String): List[Symbol] = {
      val n = ReflectOps.run(op, Array(ReflectOps.id(s)), name)
      List.tabulate(n)(i => new SymbolImpl(QuoteHost.result(i)).asInstanceOf[Symbol])
    }

    private def first(syms: List[Symbol]): Symbol = syms.headOption.getOrElse(new SymbolImpl(-1).asInstanceOf[Symbol])

    /** The declarations of `s` of kind `what` named `name`, of any name if it is empty. */
    private def declared(s: Symbol, what: String, name: String): List[Symbol] = {
      val n = ReflectOps.run("declared", Array(ReflectOps.id(s)), what + "\u0000" + name)
      List.tabulate(n)(i => new SymbolImpl(QuoteHost.result(i)).asInstanceOf[Symbol])
    }
  }

  object Signature extends SignatureModule {
    def unapply(sig: Signature): (List[String | Int], String) = (sig.paramSigs, sig.resultSig)
  }

  given SignatureMethods: SignatureMethods =
    QuoteHost.allocate("scala.quoted.runtime.impl.ReflectDeclarationSupport$ReflectDeclarations$SignatureMethodsImpl")
      .asInstanceOf[SignatureMethods]

  abstract class SignatureMethodsImpl extends SignatureMethods {
    extension (self: Signature) {
      def paramSigs: List[String | Int] = self.asInstanceOf[SignatureImpl].paramSigs
      def resultSig: String = self.asInstanceOf[SignatureImpl].resultSig
    }
  }
}
}

/** A method's signature: per parameter its erased type's name, per type parameter clause its
 *  size; its erased result type's name.
 */
final case class SignatureImpl(paramSigs: List[String | Int], resultSig: String) {
  override def toString: String = s"Signature($paramSigs,$resultSig)"
}
