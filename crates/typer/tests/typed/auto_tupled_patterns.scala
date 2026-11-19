object Pairs:
  def unapply(e: (Int, Int)): Some[Int *: Int *: EmptyTuple] = Some(e)

object Mapped:
  def unapply(x: Int): Some[Tuple.Map[(Int, Int), Option]] = Some((Some(x), None))

object Test:
  val a = (1, 2) match
    case Pairs(first, _) => first
  val b = 3 match
    case Mapped(x, y) => x
