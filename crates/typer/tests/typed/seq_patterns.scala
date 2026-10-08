object Words {
  def unapplySeq(s: String): Option[Seq[String]] = Some(s.split(" ").toSeq)
}

object Main {
  def f(xs: List[Int]): Int = xs match {
    case List(a, b) => a + b
    case List(_, _*) => 1
    case x :: rest => x + rest.length
    case _ => 0
  }
  def g(xs: Seq[Int]): Int = xs match {
    case Seq(a, rest*) => a + rest.length
    case _ => 0
  }
  def h(s: String): Int = s match {
    case Words(first, others*) => first.length + others.length
    case _ => 0
  }
  def run(): Int = f(List(1, 2)) + g(Seq(3)) + h("a b")
}
