class Wrapper[E <: Throwable]:
  object syntax:
    extension [A <: Matchable](a: E | A)
      transparent inline def fold(f: A => Int): Int =
        a match
          case e: E => 0
          case a: A => f(a)

object Throwables extends Wrapper[Throwable]
import Throwables.syntax.*

object Use:
  def x(a: Throwable | String) = a.fold(_.length)
