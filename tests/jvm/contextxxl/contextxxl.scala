type I = Int
type Mega[R] = (I, I, I, I, I, I, I, I, I, I, I, I, I, I, I, I, I, I, I, I, I, I, I) ?=> R

trait A[T]:
  def mega: Mega[T]

class B extends A[Int]:
  def mega: Mega[Int] = 42

object Contextxxl:
  def main(args: Array[String]): Unit =
    given Int = 1
    val a: A[Int] = new B
    println(a.mega)
    println((new B).mega)
