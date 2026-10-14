object Main {
  sealed trait S
  case object A extends S
  case object B extends S
  def a = Some(A).getOrElse(B)
  def b = List(A, B)
  def c = identity(identity(A))
  def d = List(Some(A), None)
  def e = Option(identity(A)).toList
  def f = identity(identity(Nil))
  def g = Some(identity(None))
  def h(o: Option[Int]) = List(o.fold(None)(_ => None))
  def i = (identity(Nil), 1)
}
