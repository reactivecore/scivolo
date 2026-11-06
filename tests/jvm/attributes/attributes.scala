trait Container[A]:
  def items: List[A]
  def first: Option[A] = items.headOption

class Box[A <: AnyRef](val value: A) extends Container[A]:
  def items: List[A] = List(value)
  def map[B <: AnyRef](f: A => B): Box[B] = Box(f(value))
  def pairs[K, V](m: Map[K, V]): List[(K, V)] = m.toList
  def arr[T <: AnyRef](xs: Array[T]): Int = xs.length

  class Inner(val n: Int):
    def owner: A = value

  object Nested:
    def twice(x: Int): Int = x * 2

class Bounded[T <: Comparable[T] & java.io.Serializable](val t: T)

@deprecated("use Box", "1.0")
class Old:
  @deprecated("gone", "1.0")
  def gone: Int = 1

object Attributes:
  val names: List[String] = List("a", "b")
  def describe(): String =
    class Local:
      def hi = "local"
    Local().hi

  def main(args: Array[String]): Unit =
    val b = Box("x")
    println(b.map(_ + "y").value)
    println(b.first)
    println(b.pairs(Map(1 -> "one")))
    println(b.arr(Array("p", "q")))
    val i = b.Inner(3)
    println(i.owner)
    println(b.Nested.twice(i.n))
    println(Bounded("s").t)
    println(describe())
    println(names)
    val r = new Runnable { def run(): Unit = println("anon") }
    r.run()
    val trace = Thread.currentThread.getStackTrace()(1)
    println(trace.getLineNumber)
