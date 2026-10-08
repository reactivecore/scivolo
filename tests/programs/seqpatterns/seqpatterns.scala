//> using scala 3.9.0
object S:
  def show(xs: List[Int]): String = xs match
    case List() => "empty"
    case List(a) => s"one $a"
    case List(1, b) => s"one-then $b"
    case List(a, b) => s"two $a $b"
    case _ => "many"

  def head(xs: Seq[String]): String = xs match
    case Seq(x, rest*) => s"$x+${rest.length}"
    case _ => "none"

  def all(xs: Seq[Int]): Int = xs match
    case Seq(rest*) => rest.sum

  def skip(xs: List[Int]): Boolean = xs match
    case List(_, _*) => true
    case _ => false

@main def seqpatterns(): Unit =
  println(S.show(Nil))
  println(S.show(List(7)))
  println(S.show(List(1, 2)))
  println(S.show(List(3, 4)))
  println(S.show(List(1, 2, 3)))
  println(S.head(Seq("a", "b", "c")))
  println(S.head(Seq()))
  println(S.all(Seq(1, 2, 3)))
  println(S.skip(Nil))
  println(S.skip(List(5)))
