final class IO[CC[X] <: Iterable[X], T](private val ord: Ordering[T]) {
  override def equals(obj: scala.Any): Boolean = obj match {
    case that: AnyRef if this eq that => true
    case that: IO[?, ?] => this.ord == that.ord
    case _ => false
  }
}
