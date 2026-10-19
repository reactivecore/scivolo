package scala.quoted.runtime.impl

/** The runs of `quotes.reflect` operations on the frontend, for the classes of `ReflectImpl`
 *  without an outer instance.
 */
object ReflectOps {
  def run(op: String, args: Array[Int], text: String): Int = QuoteHost.reflect(op, args, text)

  def results(op: String, args: Int*): List[Int] = {
    val n = run(op, args.toArray, null)
    List.tabulate(n)(QuoteHost.result)
  }

  def one(op: String, args: Int*): Int = results(op, args*).head

  /** The text operation `op` answers. */
  def text(op: String, args: Int*): String = {
    run(op, args.toArray, null)
    QuoteHost.resultText()
  }

  /** The position operation `op` answers. */
  def position(op: String, args: Int*): PositionImpl = results(op, args*) match {
    case List(file, start, end) => new PositionImpl(file, start, end)
    case _                      => throw new Unsupported("a position the frontend does not know")
  }

  def term(id: Int): Any = QuoteHost.expr(id)

  def id(x: Any): Int = x match {
    case e: ExprImpl   => QuoteHost.exprId(e)
    case t: TypeImpl   => QuoteHost.typeId(t)
    case s: SymbolImpl => s.id
    case _             => throw new Unsupported("a quotes.reflect value made outside the frontend")
  }
}

final class SymbolImpl(val id: Int) {
  override def equals(that: Any): Boolean = that match {
    case s: SymbolImpl => s.id == id
    case _             => false
  }
  override def hashCode: Int = id
}

/** Offsets `start` until `end` of source file number `file`. */
final class PositionImpl(val file: Int, val start: Int, val end: Int) {
  private lazy val lines: (List[Int], String) = {
    val n = ReflectOps.run("position", Array(file, start, end), null)
    (List.tabulate(n)(QuoteHost.result), QuoteHost.resultText())
  }

  /** Start line, start column, end line, end column: `at(0)` to `at(3)`. */
  def at(i: Int): Int = lines._1(i)

  def code: String = lines._2
}

final class SourceFileImpl(val file: Int)

/** A constant: its kind, the two words of its value, and its text if it is a string. */
final class ConstantImpl(val kind: Int, val lo: Int, val hi: Int, val text: String) {
  def value: Any = {
    val bits = (lo.toLong & 0xffffffffL) | (hi.toLong << 32)
    kind match {
      case 1  => bits.toInt
      case 2  => bits
      case 3  => bits != 0
      case 4  => bits.toChar
      case 5  => java.lang.Double.longBitsToDouble(bits)
      case 6  => java.lang.Float.intBitsToFloat(bits.toInt)
      case 7  => text
      case 8  => null
      case 9  => ()
      case 10 => bits.toByte
      case _  => bits.toShort
    }
  }
}
