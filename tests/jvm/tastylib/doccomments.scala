package scala.probe

/** A documented class, after « non-ASCII » text. */
class Documented(val n: Int):
  /** An auxiliary constructor. */
  def this() = this(0)

  /** A method. */
  // a line comment between the doc comment and its definition
  def twice: Int =
    /** A local definition. */
    val local = n * 2
    local

  /** A variable. */
  var count: Int = 0

  /** A type alias. */
  type Alias = List[Int]

  /* a plain comment */
  def plain: Int = n

/** A documented object. */
object Documented:
  /** A constant. */
  final val Zero = 0

/** A trait. */
trait Shape:
  /** An abstract member. */
  def area: Double
