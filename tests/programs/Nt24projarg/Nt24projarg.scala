//> using scala 3.9.0
trait Nt24Monad[M[_]] {
  def foo[A](a: M[A]): M[A]
}

class Nt24Bar[A, B](val b: B)
class Nt24Bar1[A] { type And[B] = Nt24Bar[A, B] }

object Nt24projarg {
  def barMonad[X]: Nt24Monad[Nt24Bar1[X]#And] = new Nt24Monad[Nt24Bar1[X]#And] {
    def foo[A](a: Nt24Bar[X, A]) = a
  }

  def main(args: Array[String]): Unit = {
    val m = barMonad[Int]
    println(m.foo(new Nt24Bar[Int, String]("ok")).b)
  }
}
