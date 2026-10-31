//> using scala 3.9.0
object Holder {
  class Box {
    private[Holder] var inner: Int = 0
    private[Holder] def twice(x: Int): Int = x * 2 + inner
  }
  class Cell[T](init: T) {
    private[Holder] var value: T = init
  }
  inline def put(b: Box, v: Int): Unit =
    b.inner = v
  inline def get(b: Box): Int = b.inner
  inline def tw(b: Box): Int = b.twice(get(b))
  inline def swap(c: Cell[String], t: String): String = {
    val cell = c
    val old = cell.value
    cell.value = t
    old
  }
  def make(): Box = new Box
}

@main def passingaccessors(): Unit = {
  val b = Holder.make()
  Holder.put(b, 3)
  println(Holder.get(b))
  println(Holder.tw(b))
  val c = new Holder.Cell("a")
  println(Holder.swap(c, "b"))
  println(Holder.swap(c, "c"))
}
