//> using scala 3.9.0
class R34Wrapper[E <: Throwable]:
  object syntax:
    extension [A <: Matchable](a: E | A)
      transparent inline def foreach(f: A => Any): Unit =
        a match
          case e: E => println("failed")
          case a: A => f(a)
    inline def twice(x: Int): Int = x + x

  def attempt[A](a: => A): E | A =
    try a
    catch case e: E => e

object R34Throwables extends R34Wrapper[Throwable]
import R34Throwables.syntax.*

object R34nestedinline:
  def main(args: Array[String]): Unit =
    for x <- R34Throwables.attempt(6 / 2) do println(x)
    for x <- R34Throwables.attempt(6 / 0) do println(x)
    println(R34Throwables.syntax.twice(21))
