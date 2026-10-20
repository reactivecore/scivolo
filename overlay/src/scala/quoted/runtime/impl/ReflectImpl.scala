// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); changed:
//   compiler/src/scala/quoted/runtime/impl/QuotesImpl.scala
package scala.quoted.runtime.impl

import scala.quoted.{Expr, Type}

/** The `reflect` of `QuotesImpl`. */
trait ReflectSupport { this: QuotesImpl =>

/** The subset of `quotes.reflect` the frontend implements: terms are `ExprImpl`s, types
 *  `TypeImpl`s, symbols `SymbolImpl`s, constants `ConstantImpl`s. The VM allocates it and the
 *  `...MethodsImpl` classes without running a constructor; a member not implemented here is no
 *  method of the class, which refuses the expansion.
 */
abstract class ReflectImpl extends reflectModule, ReflectSymbols { self: reflect.type =>

  extension (expr: Expr[Any]) def asTerm: Term = termOf(QuoteHost.exprId(expr))

  given TreeMethods: TreeMethods = methods("TreeMethodsImpl")
  given TermMethods: TermMethods = methods("TermMethodsImpl")
  given LiteralMethods: LiteralMethods = methods("LiteralMethodsImpl")
  given TypeReprMethods: TypeReprMethods = methods("TypeReprMethodsImpl")
  given SymbolMethods: SymbolMethods = methods("SymbolMethodsImpl")
  given ConstantMethods: ConstantMethods = methods("ConstantMethodsImpl")

  object Ref extends RefModule {
    def term(tp: TermRef): Ref = throw new Unsupported("Ref.term")
    def apply(sym: Symbol): Ref = termOf(one("ref", id(sym))).asInstanceOf[Ref]
  }

  object Literal extends LiteralModule {
    def apply(constant: Constant): Literal = {
      val c = constant.asInstanceOf[ConstantImpl]
      ReflectOps.run("literal", Array(c.kind, c.lo, c.hi), c.text)
      termOf(QuoteHost.result(0)).asInstanceOf[Literal]
    }
    def copy(original: Tree)(constant: Constant): Literal = throw new Unsupported("Literal.copy")
    def unapply(x: Literal): Some[Constant] = Some(x.constant)
  }

  object TypeRepr extends TypeReprModule {
    def of[T <: AnyKind](using t: Type[T]): TypeRepr = QuoteHost.tpe(QuoteHost.typeId(t)).asInstanceOf[TypeRepr]
    def typeConstructorOf(clazz: Class[?]): TypeRepr = throw new Unsupported("TypeRepr.typeConstructorOf")
  }

  object Constant extends ConstantModule
  object BooleanConstant extends BooleanConstantModule {
    def apply(x: Boolean): BooleanConstant = constant(3, if (x) 1L else 0L).asInstanceOf[BooleanConstant]
    def unapply(c: BooleanConstant): Some[Boolean] = Some(c.value.asInstanceOf[Boolean])
  }
  object ByteConstant extends ByteConstantModule {
    def apply(x: Byte): ByteConstant = constant(10, x.toLong).asInstanceOf[ByteConstant]
    def unapply(c: ByteConstant): Some[Byte] = Some(c.value.asInstanceOf[Byte])
  }
  object ShortConstant extends ShortConstantModule {
    def apply(x: Short): ShortConstant = constant(11, x.toLong).asInstanceOf[ShortConstant]
    def unapply(c: ShortConstant): Some[Short] = Some(c.value.asInstanceOf[Short])
  }
  object IntConstant extends IntConstantModule {
    def apply(x: Int): IntConstant = constant(1, x.toLong).asInstanceOf[IntConstant]
    def unapply(c: IntConstant): Some[Int] = Some(c.value.asInstanceOf[Int])
  }
  object LongConstant extends LongConstantModule {
    def apply(x: Long): LongConstant = constant(2, x).asInstanceOf[LongConstant]
    def unapply(c: LongConstant): Some[Long] = Some(c.value.asInstanceOf[Long])
  }
  object FloatConstant extends FloatConstantModule {
    def apply(x: Float): FloatConstant =
      constant(6, java.lang.Float.floatToRawIntBits(x).toLong & 0xffffffffL).asInstanceOf[FloatConstant]
    def unapply(c: FloatConstant): Some[Float] = Some(c.value.asInstanceOf[Float])
  }
  object DoubleConstant extends DoubleConstantModule {
    def apply(x: Double): DoubleConstant =
      constant(5, java.lang.Double.doubleToRawLongBits(x)).asInstanceOf[DoubleConstant]
    def unapply(c: DoubleConstant): Some[Double] = Some(c.value.asInstanceOf[Double])
  }
  object CharConstant extends CharConstantModule {
    def apply(x: Char): CharConstant = constant(4, x.toLong).asInstanceOf[CharConstant]
    def unapply(c: CharConstant): Some[Char] = Some(c.value.asInstanceOf[Char])
  }
  object StringConstant extends StringConstantModule {
    def apply(x: String): StringConstant = new ConstantImpl(7, 0, 0, x).asInstanceOf[StringConstant]
    def unapply(c: StringConstant): Some[String] = Some(c.value.asInstanceOf[String])
  }
  object UnitConstant extends UnitConstantModule {
    def apply(): UnitConstant = constant(9, 0L).asInstanceOf[UnitConstant]
    def unapply(c: UnitConstant): true = true
  }
  object NullConstant extends NullConstantModule {
    def apply(): NullConstant = constant(8, 0L).asInstanceOf[NullConstant]
    def unapply(c: NullConstant): true = true
  }

  abstract class TreeMethodsImpl extends TreeMethods {
    extension (self: Tree) {
      def asExpr: Expr[Any] = self.asInstanceOf[Expr[Any]]
      def pos: Position = ReflectOps.position("pos", ReflectOps.id(self)).asInstanceOf[Position]
    }
  }

  abstract class TermMethodsImpl extends TermMethods {
    extension (self: Term) {
      def underlying: Term = ReflectOps.term(ReflectOps.one("underlying", ReflectOps.id(self))).asInstanceOf[Term]
      def select(sym: Symbol): Select =
        ReflectOps.term(ReflectOps.one("select", ReflectOps.id(self), ReflectOps.id(sym))).asInstanceOf[Select]
      def appliedToType(targ: TypeRepr): Term = self.appliedToTypes(List(targ))
      def appliedToTypes(targs: List[TypeRepr]): Term = {
        val args = ReflectOps.id(self) :: targs.map(ReflectOps.id)
        ReflectOps.term(ReflectOps.one("appliedToTypes", args*)).asInstanceOf[Term]
      }
    }
  }

  abstract class LiteralMethodsImpl extends LiteralMethods {
    extension (self: Literal) def constant: Constant = {
      ReflectOps.run("constant", Array(ReflectOps.id(self)), null)
      new ConstantImpl(QuoteHost.result(0), QuoteHost.result(1), QuoteHost.result(2), QuoteHost.resultText())
        .asInstanceOf[Constant]
    }
  }

  abstract class TypeReprMethodsImpl extends TypeReprMethods {
    extension (self: TypeRepr) {
      def typeSymbol: Symbol = new SymbolImpl(ReflectOps.one("typeSymbol", ReflectOps.id(self))).asInstanceOf[Symbol]
      def typeArgs: List[TypeRepr] =
        ReflectOps.results("typeArgs", ReflectOps.id(self)).map(t => QuoteHost.tpe(t).asInstanceOf[TypeRepr])
    }
  }

  abstract class SymbolMethodsImpl extends SymbolMethods {
    extension (self: Symbol) {
      def companionModule: Symbol =
        new SymbolImpl(ReflectOps.one("companionModule", ReflectOps.id(self))).asInstanceOf[Symbol]
      def declaredMethod(name: String): List[Symbol] = {
        val n = ReflectOps.run("declaredMethod", Array(ReflectOps.id(self)), name)
        List.tabulate(n)(i => new SymbolImpl(QuoteHost.result(i)).asInstanceOf[Symbol])
      }
      def owner: Symbol = new SymbolImpl(ReflectOps.one("owner", ReflectOps.id(self))).asInstanceOf[Symbol]
      def name: String = ReflectOps.text("name", ReflectOps.id(self))
      def fullName: String = ReflectOps.text("fullName", ReflectOps.id(self))
      def isNoSymbol: Boolean = ReflectOps.id(self) < 0
      def exists: Boolean = ReflectOps.id(self) >= 0
    }
  }

  abstract class ConstantMethodsImpl extends ConstantMethods {
    extension (self: Constant) {
      def value: Any = self.asInstanceOf[ConstantImpl].value
      def show(using Printer[Constant]): String = throw new Unsupported("Constant.show")
    }
  }

  private def methods[T](name: String): T =
    QuoteHost.allocate("scala.quoted.runtime.impl.ReflectSupport$ReflectImpl$" + name).asInstanceOf[T]

  private def termOf(id: Int): Term = ReflectOps.term(id).asInstanceOf[Term]

  private def one(op: String, args: Int*): Int = ReflectOps.one(op, args*)

  private def id(x: Any): Int = ReflectOps.id(x)

  private def constant(kind: Int, bits: Long): Constant =
    new ConstantImpl(kind, bits.toInt, (bits >>> 32).toInt, null).asInstanceOf[Constant]
}
}
