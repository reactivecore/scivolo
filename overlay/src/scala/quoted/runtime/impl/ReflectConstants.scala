// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); changed:
//   compiler/src/scala/quoted/runtime/impl/QuotesImpl.scala
package scala.quoted.runtime.impl

import scala.reflect.TypeTest

/** The constants of `quotes.reflect`. */
trait ReflectConstantSupport { this: QuotesImpl =>

trait ReflectConstants extends reflectModule { self: reflect.type =>

  given ConstantMethods: ConstantMethods =
    QuoteHost.allocate("scala.quoted.runtime.impl.ReflectConstantSupport$ReflectConstants$ConstantMethodsImpl")
      .asInstanceOf[ConstantMethods]

  given BooleanConstantTypeTest: TypeTest[Constant, BooleanConstant] = test(3)
  given ByteConstantTypeTest: TypeTest[Constant, ByteConstant] = test(10)
  given ShortConstantTypeTest: TypeTest[Constant, ShortConstant] = test(11)
  given IntConstantTypeTest: TypeTest[Constant, IntConstant] = test(1)
  given LongConstantTypeTest: TypeTest[Constant, LongConstant] = test(2)
  given FloatConstantTypeTest: TypeTest[Constant, FloatConstant] = test(6)
  given DoubleConstantTypeTest: TypeTest[Constant, DoubleConstant] = test(5)
  given CharConstantTypeTest: TypeTest[Constant, CharConstant] = test(4)
  given StringConstantTypeTest: TypeTest[Constant, StringConstant] = test(7)
  given UnitConstantTypeTest: TypeTest[Constant, UnitConstant] = test(9)
  given NullConstantTypeTest: TypeTest[Constant, NullConstant] = test(8)

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

  abstract class ConstantMethodsImpl extends ConstantMethods {
    extension (self: Constant) {
      def value: Any = self.asInstanceOf[ConstantImpl].value
      def show(using Printer[Constant]): String = throw new Unsupported("Constant.show")
    }
  }

  private def constant(kind: Int, bits: Long): Constant =
    new ConstantImpl(kind, bits.toInt, (bits >>> 32).toInt, null).asInstanceOf[Constant]

  private def test[T](kind: Int): T = new ConstantKindTest(kind).asInstanceOf[T]
}
}

/** The type test of constants of kind `kind`. */
final class ConstantKindTest(kind: Int) extends TypeTest[Any, Any] {
  def unapply(x: Any): Option[x.type & Any] = x match {
    case c: ConstantImpl if c.kind == kind => Some(x)
    case _                                 => None
  }
}
