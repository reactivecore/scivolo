object GadtCaseTypeLeveled {
  case class One[T](fst: T)

  def result[T](e: One[T]) = e match {
    case foo: One[a] =>
      val ok: a = e.fst
      One(ok)
  }

  def nested[T](e: One[T]) = {
    val x = e match {
      case foo: One[a] =>
        val ok: a = e.fst
        One(ok)
    }
    x
  }

  def covariant[T](e: One[T]) = e match {
    case foo: One[a] =>
      val ok: a = e.fst
      List(ok)
  }

  val one: One[Int] = result(One(0))
}
