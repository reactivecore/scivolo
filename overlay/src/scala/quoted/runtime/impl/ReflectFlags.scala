// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); changed:
//   compiler/src/scala/quoted/runtime/impl/QuotesImpl.scala
package scala.quoted.runtime.impl

/** The flags and `defn` of `quotes.reflect`. */
trait ReflectFlagSupport { this: QuotesImpl =>

/** A flag is the bit of its TASTy modifier tag; bits 2 to 5 are `Method`, `Package`, `Param` and
 *  `AbsOverride`, bit 50 `JavaDefined`, bit 51 `Scala2x`, and bit 52 flags no symbol has. Flags
 *  the frontend does not know are `Unsupported`.
 */
trait ReflectFlags extends reflectModule { self: reflect.type =>

  object Flags extends FlagsModule {
    def Abstract: Flags = tag(9)
    def AbsOverride: Flags = tag(5)
    def Artifact: Flags = tag(24)
    def Case: Flags = tag(12)
    def CaseAccessor: Flags = tag(27)
    def Contravariant: Flags = tag(29)
    def Covariant: Flags = tag(28)
    def Deferred: Flags = refused("Flags.Deferred")
    def EmptyFlags: Flags = new FlagsImpl(0L).asInstanceOf[Flags]
    def Enum: Flags = tag(21)
    def Erased: Flags = tag(34)
    def Exported: Flags = tag(39)
    def ExtensionMethod: Flags = tag(36)
    def FieldAccessor: Flags = tag(26)
    def Final: Flags = tag(10)
    def Given: Flags = tag(37)
    def HasDefault: Flags = tag(31)
    def Implicit: Flags = tag(13)
    def Infix: Flags = tag(43)
    def Inline: Flags = tag(17)
    def Into: Flags = tag(49)
    def Invisible: Flags = tag(44)
    def JavaDefined: Flags = tag(50)
    def JavaStatic: Flags = tag(18)
    def JavaAnnotation: Flags = tag(52)
    def Lazy: Flags = tag(14)
    def Local: Flags = tag(22)
    def Macro: Flags = tag(33)
    def Method: Flags = tag(2)
    def Module: Flags = tag(19)
    def Mutable: Flags = tag(25)
    def NoInits: Flags = refused("Flags.NoInits")
    def Opaque: Flags = tag(35)
    def Open: Flags = tag(40)
    def Override: Flags = tag(15)
    def Package: Flags = tag(3)
    def Param: Flags = tag(4)
    def ParamAccessor: Flags = tag(38)
    def Private: Flags = tag(6)
    def PrivateLocal: Flags = tag(6, 22)
    def Protected: Flags = tag(8)
    def Scala2x: Flags = tag(51)
    def Sealed: Flags = tag(11)
    def StableRealizable: Flags = refused("Flags.StableRealizable")
    def Static: Flags = tag(18)
    def Synthetic: Flags = tag(23)
    def Tracked: Flags = tag(47)
    def Trait: Flags = tag(20)
    def Transparent: Flags = tag(42)
  }

  given FlagsMethods: FlagsMethods = QuoteHost
    .allocate("scala.quoted.runtime.impl.ReflectFlagSupport$ReflectFlags$FlagsMethodsImpl")
    .asInstanceOf[FlagsMethods]

  abstract class FlagsMethodsImpl extends FlagsMethods {
    extension (self: Flags) {
      def is(that: Flags): Boolean = (FlagsImpl.bits(self) & FlagsImpl.bits(that)) == FlagsImpl.bits(that)
      def |(that: Flags): Flags = new FlagsImpl(FlagsImpl.bits(self) | FlagsImpl.bits(that)).asInstanceOf[Flags]
      def &(that: Flags): Flags = new FlagsImpl(FlagsImpl.bits(self) & FlagsImpl.bits(that)).asInstanceOf[Flags]
      def show: String = throw new Unsupported("Flags.show")
    }
  }

  object defn extends defnModule {
    def RootPackage: Symbol = new SymbolImpl(ReflectOps.one("root")).asInstanceOf[Symbol]
    def RootClass: Symbol = RootPackage
    def EmptyPackageClass: Symbol = new SymbolImpl(ReflectOps.one("emptyPackage")).asInstanceOf[Symbol]
    def ScalaPackage: Symbol = Symbol.requiredPackage("scala")
    def ScalaPackageClass: Symbol = Symbol.requiredPackage("scala")
    def AnyClass: Symbol = Symbol.requiredClass("scala.Any")
    def MatchableClass: Symbol = Symbol.requiredClass("scala.Matchable")
    def AnyValClass: Symbol = Symbol.requiredClass("scala.AnyVal")
    def ObjectClass: Symbol = Symbol.requiredClass("java.lang.Object")
    def AnyRefClass: Symbol = refused("defn.AnyRefClass")
    def NullClass: Symbol = Symbol.requiredClass("scala.Null")
    def NothingClass: Symbol = Symbol.requiredClass("scala.Nothing")
    def UnitClass: Symbol = Symbol.requiredClass("scala.Unit")
    def ByteClass: Symbol = Symbol.requiredClass("scala.Byte")
    def ShortClass: Symbol = Symbol.requiredClass("scala.Short")
    def CharClass: Symbol = Symbol.requiredClass("scala.Char")
    def IntClass: Symbol = Symbol.requiredClass("scala.Int")
    def LongClass: Symbol = Symbol.requiredClass("scala.Long")
    def FloatClass: Symbol = Symbol.requiredClass("scala.Float")
    def DoubleClass: Symbol = Symbol.requiredClass("scala.Double")
    def BooleanClass: Symbol = Symbol.requiredClass("scala.Boolean")
    def StringClass: Symbol = Symbol.requiredClass("java.lang.String")
    def ClassClass: Symbol = Symbol.requiredClass("java.lang.Class")
    def ArrayClass: Symbol = Symbol.requiredClass("scala.Array")
    def PredefModule: Symbol = Symbol.requiredModule("scala.Predef")
    def Predef_classOf: Symbol = new SymbolImpl(ReflectOps.one("predefClassOf")).asInstanceOf[Symbol]
    def JavaLangPackage: Symbol = Symbol.requiredPackage("java.lang")
    def ArrayModule: Symbol = Symbol.requiredModule("scala.Array")
    def Array_apply: Symbol = Symbol.requiredMethod("scala.Array.apply")
    def Array_clone: Symbol = Symbol.requiredMethod("scala.Array.clone")
    def Array_length: Symbol = Symbol.requiredMethod("scala.Array.length")
    def Array_update: Symbol = Symbol.requiredMethod("scala.Array.update")
    def RepeatedParamClass: Symbol = Symbol.requiredClass("scala.<repeated>")
    def RepeatedAnnot: Symbol = Symbol.requiredClass("scala.annotation.internal.Repeated")
    def OptionClass: Symbol = Symbol.requiredClass("scala.Option")
    def NoneModule: Symbol = Symbol.requiredModule("scala.None")
    def SomeModule: Symbol = Symbol.requiredModule("scala.Some")
    def ProductClass: Symbol = Symbol.requiredClass("scala.Product")
    def FunctionClass(arity: Int, isImplicit: Boolean = false, isErased: Boolean = false): Symbol =
      if (isErased) refused("defn.FunctionClass of an erased function") else FunctionClass(arity, isImplicit)
    def FunctionClass(arity: Int): Symbol = Symbol.requiredClass(s"scala.Function$arity")
    def FunctionClass(arity: Int, isContextual: Boolean): Symbol =
      Symbol.requiredClass(if (isContextual) s"scala.ContextFunction$arity" else s"scala.Function$arity")
    def PolyFunctionClass: Symbol = Symbol.requiredClass("scala.PolyFunction")
    def TupleClass(arity: Int): Symbol = Symbol.requiredClass(s"scala.Tuple$arity")
    def isTupleClass(sym: Symbol): Boolean =
      ReflectOps.id(sym) >= 0 && sym.isClassDef && (1 to 22).exists(n => sym.fullName == s"scala.Tuple$n")
    def ScalaPrimitiveValueClasses: List[Symbol] = UnitClass :: BooleanClass :: ScalaNumericValueClasses
    def ScalaNumericValueClasses: List[Symbol] =
      List(ByteClass, ShortClass, CharClass, IntClass, LongClass, FloatClass, DoubleClass)
  }

  private def tag(bits: Int*): Flags = new FlagsImpl(bits.foldLeft(0L)((f, b) => f | 1L << b)).asInstanceOf[Flags]

  private def refused(what: String): Nothing = throw new Unsupported(what)
}
}

final class FlagsImpl(val bits: Long) {
  override def equals(that: Any): Boolean = that match {
    case f: FlagsImpl => f.bits == bits
    case _            => false
  }
  override def hashCode: Int = java.lang.Long.hashCode(bits)
  override def toString: String = throw new Unsupported("Flags.toString")
}

object FlagsImpl {
  def bits(f: Any): Long = f.asInstanceOf[FlagsImpl].bits
}
