import scala.language.implicitConversions

class Wrap[T](val t: T)
trait Ev[T]
object Ev:
  implicit def viaView[T](implicit f: T => Wrap[T]): Ev[T] = ???

object C:
  implicit def toWrap[T](x: T)(implicit ev: Ev[T]): Wrap[T] = new Wrap(x)

class K
object K:
  extension (k: K)
    def go(f: Int => String): Int = f(1).length
    def plain: Int = 2

import C.toWrap

object Test:
  val a = new K().go(i => i.toString)
  val b = new K().plain
