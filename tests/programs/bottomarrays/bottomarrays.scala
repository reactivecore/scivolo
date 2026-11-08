trait Named { def name: String = "named" }
class Plain
trait SelfCast { self: Named =>
  def me: Named = this.asInstanceOf[Named]
  def asPlain: Plain = this.asInstanceOf[Plain]
}
class Both extends Named with SelfCast

object bottomarrays {
  def nothings: Array[Nothing] = new Array[Nothing](0)
  def nulls(n: Int): Array[Null] = new Array[Null](n)
  def take(a: Array[Nothing]): Int = a.length
  def take(a: Array[Null]): Int = a.length + 100
  def generic[T](a: Array[T]): Int = a.length

  def main(args: Array[String]): Unit = {
    val a = nothings
    val b = nulls(2)
    b(1) = null
    println((a.length, b.length, b(0), b.toList))
    println((take(a), take(b), generic(b)))
    println(b.asInstanceOf[Array[AnyRef]].length)
    println(b.isInstanceOf[Array[AnyRef]])
    println(new Array[Nothing](3).getClass.getName)
    val both = new Both
    println(both.me.name)
    try println(both.asPlain)
    catch { case _: ClassCastException => println("ClassCastException") }
  }
}
