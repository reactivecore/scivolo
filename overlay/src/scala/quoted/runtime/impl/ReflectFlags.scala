// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); changed:
//   compiler/src/scala/quoted/runtime/impl/QuotesImpl.scala
package scala.quoted.runtime.impl

/** The flags and `defn` of `quotes.reflect`. */
trait ReflectFlagSupport { this: QuotesImpl =>

/** A flag is the bit of its TASTy modifier tag; bits 2 to 5 are `Method`, `Package`, `Param` and
 *  `AbsOverride`, bit 51 `Scala2x`, and bit 50 flags no symbol has. Flags the frontend does not
 *  know are `Unsupported`.
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
    def JavaAnnotation: Flags = tag(50)
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
    def ParamAccessor: Flags = refused("Flags.ParamAccessor")
    def Private: Flags = tag(6)
    def PrivateLocal: Flags = tag(6, 22)
    def Protected: Flags = tag(8)
    def Scala2x: Flags = tag(51)
    def Sealed: Flags = tag(11)
    def StableRealizable: Flags = tag(32)
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
    def EmptyPackageClass: Symbol = refused("defn.EmptyPackageClass")
    def ScalaPackage: Symbol = refused("defn.ScalaPackage")
    def ScalaPackageClass: Symbol = refused("defn.ScalaPackageClass")
    def AnyClass: Symbol = refused("defn.AnyClass")
    def MatchableClass: Symbol = refused("defn.MatchableClass")
    def AnyValClass: Symbol = refused("defn.AnyValClass")
    def ObjectClass: Symbol = refused("defn.ObjectClass")
    def AnyRefClass: Symbol = refused("defn.AnyRefClass")
    def NullClass: Symbol = refused("defn.NullClass")
    def NothingClass: Symbol = refused("defn.NothingClass")
    def UnitClass: Symbol = refused("defn.UnitClass")
    def ByteClass: Symbol = refused("defn.ByteClass")
    def ShortClass: Symbol = refused("defn.ShortClass")
    def CharClass: Symbol = refused("defn.CharClass")
    def IntClass: Symbol = refused("defn.IntClass")
    def LongClass: Symbol = refused("defn.LongClass")
    def FloatClass: Symbol = refused("defn.FloatClass")
    def DoubleClass: Symbol = refused("defn.DoubleClass")
    def BooleanClass: Symbol = refused("defn.BooleanClass")
    def StringClass: Symbol = refused("defn.StringClass")
    def ClassClass: Symbol = refused("defn.ClassClass")
    def ArrayClass: Symbol = refused("defn.ArrayClass")
    def PredefModule: Symbol = refused("defn.PredefModule")
    def Predef_classOf: Symbol = refused("defn.Predef_classOf")
    def JavaLangPackage: Symbol = refused("defn.JavaLangPackage")
    def ArrayModule: Symbol = refused("defn.ArrayModule")
    def Array_apply: Symbol = refused("defn.Array_apply")
    def Array_clone: Symbol = refused("defn.Array_clone")
    def Array_length: Symbol = refused("defn.Array_length")
    def Array_update: Symbol = refused("defn.Array_update")
    def RepeatedParamClass: Symbol = refused("defn.RepeatedParamClass")
    def RepeatedAnnot: Symbol = refused("defn.RepeatedAnnot")
    def OptionClass: Symbol = refused("defn.OptionClass")
    def NoneModule: Symbol = refused("defn.NoneModule")
    def SomeModule: Symbol = refused("defn.SomeModule")
    def ProductClass: Symbol = refused("defn.ProductClass")
    def FunctionClass(arity: Int, isImplicit: Boolean = false, isErased: Boolean = false): Symbol =
      refused("defn.FunctionClass")
    def FunctionClass(arity: Int): Symbol = refused("defn.FunctionClass")
    def FunctionClass(arity: Int, isContextual: Boolean): Symbol = refused("defn.FunctionClass")
    def PolyFunctionClass: Symbol = refused("defn.PolyFunctionClass")
    def TupleClass(arity: Int): Symbol = refused("defn.TupleClass")
    def isTupleClass(sym: Symbol): Boolean = refused("defn.isTupleClass")
    def ScalaPrimitiveValueClasses: List[Symbol] = refused("defn.ScalaPrimitiveValueClasses")
    def ScalaNumericValueClasses: List[Symbol] = refused("defn.ScalaNumericValueClasses")
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
}

object FlagsImpl {
  def bits(f: Any): Long = f.asInstanceOf[FlagsImpl].bits
}
