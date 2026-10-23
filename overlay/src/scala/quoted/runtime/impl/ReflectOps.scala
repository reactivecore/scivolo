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

  /** The flags of symbol `sym`. */
  def flags(sym: Int): FlagsImpl = results("flags", sym) match {
    case List(lo, hi) => new FlagsImpl((lo.toLong & 0xffffffffL) | (hi.toLong << 32))
    case _            => throw new Unsupported("flags the frontend does not know")
  }

  /** The kind of tree `x`: 1 `Ident`, 2 `Select`, 3 `Literal`, 4 `Typed`, 5 `Apply`, 6 `TypeApply`,
   *  7 `Block`, 8 `Inlined`, 9 `Repeated`, 10 `TypeTree`, 11 `Inferred`, 12 `ValDef`, 13 `NamedArg`,
   *  14 another term, 15 `DefDef`, 16 `TypeDef`, 17 `Import`, 18 `This`, 0 another tree.
   */
  def kind(x: Any): Int = one("treeKind", id(x))

  /** The kind of tree `x`, its children in the extractor's order and its name. */
  def shape(x: Any): Shape = {
    val n = run("tree", Array(id(x)), null)
    new Shape(QuoteHost.result(0), List.tabulate(n - 1)(i => QuoteHost.result(i + 1)), QuoteHost.resultText())
  }

  def term(id: Int): Any = QuoteHost.expr(id)

  /** Expression `id`, if there is one. */
  def opt(id: Int): Option[Any] = if (id < 0) None else Some(term(id))

  def id(x: Any): Int = x match {
    case e: ExprImpl   => QuoteHost.exprId(e)
    case t: TypeImpl   => QuoteHost.typeId(t)
    case s: SymbolImpl => s.id
    case _             => throw new Unsupported("a quotes.reflect value made outside the frontend")
  }
}

final class Shape(val kind: Int, val children: List[Int], val name: String) {
  def child(i: Int): Any = ReflectOps.term(children(i))

  /** The children from the `i`th on. */
  def from(i: Int): List[Any] = children.drop(i).map(ReflectOps.term)
}

/** The type test of trees of kinds `kinds`. */
final class KindTest(kinds: Int*) extends scala.reflect.TypeTest[Any, Any] {
  def unapply(x: Any): Option[x.type & Any] = if (kinds.contains(ReflectOps.kind(x))) Some(x) else None
}

final class SymbolImpl(val id: Int) {
  override def equals(that: Any): Boolean = that match {
    case s: SymbolImpl => s.id == id
    case _             => false
  }
  override def hashCode: Int = id
  override def toString: String = throw new Unsupported("Symbol.toString")
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

  override def toString: String = throw new Unsupported("Position.toString")
}

final class SourceFileImpl(val file: Int) {
  override def toString: String = throw new Unsupported("SourceFile.toString")
}

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

  override def toString: String = throw new Unsupported("Constant.toString")
}
