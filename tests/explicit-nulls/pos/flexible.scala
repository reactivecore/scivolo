//> using options -Yexplicit-nulls --fresh -classpath libs/javalib-tasty-0.5.12.jar
object Flexible:
  val prop = System.getProperty("a")
  val sub = "abc".substring(1)
  val len = sub.length
  val upper = "abc".substring(1).toUpperCase
  val nn = sub.nn
  val trimmed: String = sub.trim
  val eq = sub == null
  val joined = String.join(",", "a", "b")
  val obj: Object = new Object
  val same = obj.equals(sub)
  def f(x: CharSequence | Null): Int = 0
  val g = f(sub)
  val s: String = System.getProperty("b")
  val n: String | Null = System.getProperty("c")
  def id(x: String | Null): String | Null = x
  val passed = id(System.getProperty("d"))
  val arr = Array(sub)
  val formatted = String.format("%s %s", 1, sub)
  val orNull = if eq then sub else null

class Ordered1 extends Comparable[Ordered1]:
  def compareTo(o: Ordered1): Int = 0
  override def clone(): Object = this
  val cmp = compareTo(this)
  val cls = getClass
