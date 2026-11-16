class Outer[U] {
  type T <: U
  def mk(n: Int): Array[T] = new Array[AnyRef](n).asInstanceOf[Array[T]]
}

object Memberarrays {
  def viaPrefix(o: Outer[String]): Array[o.T] = o.mk(1)

  def local(o: Outer[String]): Int = {
    val a: Array[o.T] = o.mk(2)
    a.length
  }

  def cast(f: => Int): String =
    try f.toString
    catch { case _: ClassCastException => "ClassCastException" }

  def main(args: Array[String]): Unit = {
    val o = new Outer[String]
    println(cast(viaPrefix(o).length))
    println(cast(local(o)))
    println(cast(new Outer[Int].mk(3).length))
    println(new Outer[AnyRef].mk(4).length)
  }
}
