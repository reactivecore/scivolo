package forwarderflags

trait Ord[T] extends java.util.Comparator[T]:
  outer =>
  val tag: String = "ord"
  def orElse(other: Ord[T]): Ord[T] = (x, y) =>
    val r = outer.compare(x, y)
    if r != 0 then r else other.compare(x, y)
