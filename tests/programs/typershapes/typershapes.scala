//> using options -Yexplicit-nulls
import language.experimental.captureChecking

object Helpers {
  def twice(n: Int): Int = n * 2
}

abstract class Gen[+A] { def next(): A }

object Gens {
  def empty[A]: Gen[A] = new Gen[A] { def next(): A = throw new NoSuchElementException }
  def fill[A](len: Int)(elem: => A): Gen[A]^{elem} = new Gen[A] {
    private var i = 0
    import Helpers.*
    def next(): A = if (i < twice(len)) { i += 1; elem } else empty.next()
  }
}

class Dup[A](xs: List[A]) {
  def duplicate: (Iterator[A]^{this}, Iterator[A]^{this}) = (xs.iterator, xs.iterator)
  def partition(p: A => Boolean): (Iterator[A]^{this, p}, Iterator[A]^{this, p}) = {
    val (a, b) = duplicate
    (a filter p, b filterNot p)
  }
}

object typershapes {
  def later(f: => Iterator[Int]): Iterator[Int] = f

  def firstNonEmpty(xs: List[Int], f: Int => IterableOnce[Int]): Iterator[Int] = {
    var it: Iterator[Int] | Null = null
    var has = false
    var rest = xs
    while (!has && rest.nonEmpty) {
      it = f(rest.head).iterator
      has = it.hasNext
      if (!has) rest = rest.tail
    }
    if (has) later(it.nn) else Iterator.empty
  }

  def main(args: Array[String]): Unit = {
    println(firstNonEmpty(List(0, 2, 3), n => List.fill(n)(n)).toList)
    println(Gens.fill(1)("x").next())
    val (big, small) = new Dup(List(1, 2, 3)).partition(_ > 1)
    println((big.toList, small.toList))
  }
}
