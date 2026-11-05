class Tree[K] {
  private type Nd = Tree.Nd[K]
  def build(n: Int): Nd = {
    def f(i: Int, acc: Int): Nd = if (i == 0) new Nd(acc) else f(i - 1, acc + i)
    f(n, 0)
  }
}

object Tree {
  final class Nd[K](val sum: Int)
}

trait Base[+A, +CC[_]] {
  def concat[B >: A](suffix: List[B]): CC[B] = ???
}

trait Ops[+A, +CC[_]] extends Base[A, CC] {
  override def concat[B >: A](suffix: List[B]): CC[B] = super.concat(suffix)
}

class Impl extends Base[Int, List] {
  override def concat[B >: Int](suffix: List[B]): List[B] = 0 :: suffix
}

class Mixed extends Impl with Ops[Int, List]

@main def staticLocal(): Unit = {
  def twice(x: Int): Int = x * 2
  println(new Tree[String]().build(4).sum)
  println(twice(21))
  val b: Base[Int, List] = new Mixed
  println(b.concat(List(1, 2)))
}
