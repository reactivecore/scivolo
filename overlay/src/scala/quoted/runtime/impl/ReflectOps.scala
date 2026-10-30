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

  /** The kind of tree `x`, as `TreeKinds` numbers them. */
  def kind(x: Any): Int = one("treeKind", id(x))

  /** `X.copy(original)(parts)`: the original if `parts` and `name` are its own. A part is a
   *  tree, an optional tree or a list of trees, in the order of the original's extractor.
   */
  def copy(original: Any, name: String, parts: Any*): Any = {
    val ids = parts.toList.flatMap {
      case None        => List(-1)
      case Some(x)     => List(id(x))
      case xs: List[?] => xs.map(id)
      case x           => List(id(x))
    }
    run("copy", (id(original) :: ids).toArray, name)
    term(QuoteHost.result(0))
  }

  /** The kind of tree `x`, its children in the extractor's order and its name. */
  def shape(x: Any): Shape = {
    val n = run("tree", Array(id(x)), null)
    new Shape(QuoteHost.result(0), List.tabulate(n - 1)(i => QuoteHost.result(i + 1)), QuoteHost.resultText())
  }

  /** `ValDef.copy` and `DefDef.copy` of `original` with parameters `params`. */
  def copyDef(original: Any, name: String, params: List[Any], tpt: Any, rhs: Option[Any]): Any = {
    val args = Array(id(original), id(tpt), rhs.fold(-1)(id)) ++ params.map(id)
    run("copyDef", args, name)
    term(QuoteHost.result(0))
  }

  /** Child `i` of tree `x`, and its children from the `i`th on. */
  def part[T](x: Any, i: Int): T = shape(x).child(i).asInstanceOf[T]

  def from[T](x: Any, i: Int): List[T] = shape(x).from(i).asInstanceOf[List[T]]

  /** The type of tree `x` and the kind of its widened type (`TypeShapes.kind`). */
  def tpe(x: Any): Any = QuoteHost.tpe(one("tpe", id(x)))

  def widenedKind(x: Any): Int = TypeShapes.kind(QuoteHost.tpe(one("widen", id(tpe(x)))))

  /** `Tree.isExpr`: whether `x` is a term not of a method or polymorphic type. */
  def isExpr(x: Any): Boolean = TreeKinds.terms.contains(kind(x)) && widenedKind(x) != 8 && widenedKind(x) != 9

  def symbol(x: Any): Any = new SymbolImpl(one("treeSymbol", id(x)))

  def term(id: Int): Any = QuoteHost.tree(id)

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

/** The kinds of trees: 1 `Ident`, 2 `Select`, 3 `Literal`, 4 `Typed`, 5 `Apply`, 6 `TypeApply`,
 *  7 `Block`, 8 `Inlined`, 9 `Repeated`, 11 `Inferred`, 12 `ValDef`,
 *  13 `NamedArg`, 14 another term, 15 `DefDef`, 16 `TypeDef`, 17 `Import`, 18 `This`, 19 `ClassDef`,
 *  20 `Closure`, 21 `Super`, 22 `New`, 23 `Assign`, 24 `If`, 25 `Match`, 26 `SummonFrom`,
 *  27 `Return`, 28 `While`, 29 `Try`, 30 `CaseDef`, 31 `TypeCaseDef`, 32 `Bind`, 33 `TypeBind`,
 *  34 `Unapply`, 35 `Alternatives`, 36 `TypedOrTest`, 37 `TypeIdent`, 38 `TypeSelect`,
 *  39 `TypeProjection`, 40 `Singleton`, 41 `Refined`, 42 `Applied`, 43 `Annotated`, 44 `ByName`,
 *  45 `LambdaTypeTree`, 46 `TypeBoundsTree`, 47 `WildcardTypeTree`, 48 `Export`,
 *  49 `PackageClause`, 50 `TypeBlock`, 0 another tree.
 */
object TreeKinds {
  val terms: Seq[Int] = Seq(1, 2, 3, 4, 5, 6, 7, 8, 9, 13, 14, 18, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29)
  val definitions: Seq[Int] = Seq(12, 15, 16, 19)
  val statements: Seq[Int] = terms ++ definitions ++ Seq(17, 48)
  val typeTrees: Seq[Int] = Seq(11, 33, 37, 38, 39, 40, 41, 42, 43, 44, 45, 47, 50)
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

  /** dotty's `SymDenotation.toString`: its kind and name. */
  override def toString: String =
    if (id < 0) "val <none>"
    else {
      val bits = ReflectOps.flags(id).bits
      val has = (bit: Int) => (bits & 1L << bit) != 0
      val kind = ReflectOps.one("kind", id) match {
        case 1 if has(19)      => "module class"
        case 1 if has(20)      => "trait"
        case 1                 => "class"
        case 2 | 3             => "type"
        case 0                 => "object"
        case _ if has(19)      => "object"
        case 6                 => "method"
        case _                 => "val"
      }
      kind + " " + ReflectOps.text("name", id)
    }
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
