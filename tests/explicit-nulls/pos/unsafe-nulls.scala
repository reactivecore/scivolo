//> using options -Yexplicit-nulls --fresh
import scala.language.unsafeNulls

object UnsafeNulls:
  val s: String = null
  val n: String | Null = "a"
  val t: String = n
  val len = n.length
  val c = n.charAt(0)
  def f(x: String | Null): Int = x.length + 1
  def g(x: String | Null) = x.isEmpty
  val xs: List[String] = List(null, "b")
  def h(x: String | Null): Boolean = x == "c"
  val arr: Array[String] = Array(null)
