class Inv[A, X](var a: A, val x: X)
class HBox[F[_]](var v: F[Int])
class Sub[A](init: Inv[A, Int]) extends HBox[[X] =>> Inv[A, X]](init)

object WildcardLambda {
  def bad(s: Sub[?]): HBox[[X] =>> Inv[?, X]] = s
}
