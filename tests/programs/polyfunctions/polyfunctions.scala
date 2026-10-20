// Polymorphic function types and values.
type Op[T, C] = T => C => C
type Lst[T] = [C] => Op[T, C] => C => C

def nil[T]: Lst[T] = [C] => (op: Op[T, C]) => (s: C) => s

def cons[T](hd: T, tl: Lst[T]): Lst[T] =
  [C] => (op: Op[T, C]) => (s: C) => op(hd)(tl(op)(s))

def toList[T](xs: Lst[T]): List[T] = xs[List[T]](h => t => h :: t)(Nil)

def headOf(f: [T] => List[T] => Option[T]): Option[Int] = f(List(1, 2, 3))

def viaBound[S <: Any & ([V] => V => Int)](x: S): Int = x[String]("abc")

object polyfunctions:
  val first = [T] => (ts: List[T]) => ts.headOption
  val pair = [A, B] => (a: A, b: B) => (b, a)
  val sized: [V] => V => Int = [V] => (v: V) => v.toString.length

  def main(args: Array[String]): Unit =
    println(first(List("a", "b")))
    println(headOf(first))
    println(pair(1, "one"))
    println(toList(cons(1, cons(2, nil))))
    println(sized(12345))
    println(viaBound([V] => (v: V) => v.hashCode % 7))
