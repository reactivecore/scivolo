package scalai.repl

import scala.collection.immutable.{HashMap, HashSet}
import scala.collection.mutable.ArraySeq

/** Values as dotty's REPL prints them, after its vendored pprint without colors. */
object Show {
  private val forcedToString = Set("scala.collection.mutable.StringBuilder", "scala.xml.Elem")
  private val lazyClassNames = Set(
    "scala.collection.immutable.Stream$Cons",
    "scala.collection.immutable.LazyList",
    "scala.collection.immutable.LazyListIterable"
  )
  private val tuplePrefix = "scala.Tuple"

  /** `x` wrapped at `width` columns, starting at column `offset`, cut off after `height` lines. */
  def apply(x: Any, width: Int, height: Int, offset: Int): String =
    if (x != null && forcedToString(x.getClass.getName)) x.toString
    else new Truncated(new Renderer(width, 2).rec(treeify(x), offset, 0).iter, width, height).mkString

  /** Whether the `toString` of `x`'s class is neither `Object`'s nor a case class's synthesized one. */
  def hasUserToString(x: Any): Boolean = throw new UnsupportedOperationException("VM native")

  /** `x.collectionClassName`. */
  def collectionName(x: Iterable[?]): String = throw new UnsupportedOperationException("VM native")

  private def toStringTree(x: Any) = Tree.Lazy { () =>
    val s = x.toString
    if (s == null) "null" else s
  }

  def treeify(x: Any): Tree = x match {
    case null       => Tree.Literal("null")
    case x: Boolean => Tree.Literal(x.toString)
    case x: Char =>
      val sb = new StringBuilder
      sb.append('\'')
      Util.escapeChar(x, sb)
      sb.append('\'')
      Tree.Literal(sb.toString)
    case x: Byte   => Tree.Literal(x.toString)
    case x: Short  => Tree.Literal(x.toString)
    case x: Int    => Tree.Literal(x.toString)
    case x: Long   => Tree.Literal(x.toString + "L")
    case x: Float  => Tree.Literal(x.toString + "F")
    case x: Double => Tree.Literal(x.toString)
    case x: String =>
      if (x.exists(c => c == '\n' || c == '\r')) Tree.Literal("\"\"\"" + x + "\"\"\"")
      else Tree.Literal(Util.literalize(x))
    case x: StringBuilder => treeify(x.toString)
    case x: Symbol        => Tree.Literal("'" + x.name)
    case x: scala.collection.Map[?, ?] =>
      val prefix = x match {
        case _: HashMap[?, ?] => "Map"
        case _                => collectionName(x)
      }
      Tree.Apply(prefix, x.iterator.map((k, v) => Tree.Infix(treeify(k), "->", treeify(v))))
    case x: Iterable[?] if lazyClassNames(x.getClass.getName) && x.toString.contains("<not computed>") =>
      Tree.Literal(x.toString)
    case x: Iterable[?] =>
      val prefix = x match {
        case _: ArraySeq[?] => "Seq"
        case _: HashSet[?]  => "Set"
        case _              => collectionName(x)
      }
      Tree.Apply(prefix, x.iterator.map(treeify))
    case None            => Tree.Literal("None")
    case it: Iterator[?] => Tree.Literal(if (it.isEmpty) "empty iterator" else "non-empty iterator")
    case x: Array[?]     => Tree.Apply("Array", x.iterator.map(treeify))
    case x: Product      => product(x)
    case x               => toStringTree(x)
  }

  private def product(x: Product): Tree = {
    val className = x.getClass.getName
    val isTuple = className.startsWith(tuplePrefix)
    if (x.productArity == 0) Tree.Lazy(() => x.toString)
    else if (!isTuple && hasUserToString(x)) toStringTree(x)
    else if (x.productArity == 2 && Util.isOperator(x.productPrefix))
      Tree.Infix(treeify(x.productElement(0)), x.productPrefix, treeify(x.productElement(1)))
    else if (isTuple && className.lift(tuplePrefix.length).exists(c => c >= '2' && c <= '9'))
      Tree.Apply("", x.productIterator.map(treeify))
    else Tree.Apply(x.productPrefix, elements(x))
  }

  /** The elements of `x`, as `name = value` if it has two or more. */
  private def elements(x: Product): Iterator[Tree] =
    if (x.productArity < 2) x.productIterator.map(treeify)
    else
      x.productElementNames.zipWithIndex.map { (name, i) =>
        val key = if (Util.isIdentifier(name)) name else s"`$name`"
        Tree.KeyValue(key, treeify(x.productElement(i)))
      }
}
