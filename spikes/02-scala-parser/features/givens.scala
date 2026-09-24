trait Ord[T]:
  def compare(a: T, b: T): Int

given Ord[Int] with
  def compare(a: Int, b: Int): Int = a - b

given intOrd: Ord[Int] = (a, b) => a - b

given listOrd[T](using ord: Ord[T]): Ord[List[T]] with
  def compare(a: List[T], b: List[T]): Int = 0

given [T: Ord] => Ord[Option[T]]:
  def compare(a: Option[T], b: Option[T]): Int = 0

given Ord[String] = _.compareTo(_)

def max[T](a: T, b: T)(using ord: Ord[T]): T = if ord.compare(a, b) > 0 then a else b
def max2[T: Ord as ord](a: T, b: T): T = a
def sorted[T: {Ord, Ordering}](xs: List[T]): List[T] = xs
val m = max(1, 2)(using intOrd)

import scala.math.Ordering.given
import scala.concurrent.ExecutionContext.{given, *}

type Ctx[T] = Ord[Int] ?=> T
def withCtx: Ctx[Int] = summon[Ord[Int]].compare(1, 2)
