object NestedInlineArgSkolems:
  transparent inline def sm[T](using x: T): x.type = x
  transparent inline def outer: Option[Int] = sm[Option[Int]](using Option(1))
  def nested = outer
