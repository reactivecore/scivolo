import scala.annotation.tailrec
import scala.collection.mutable.ListBuffer

final class Node(val v: Int, val next: Node | Null) {
  @tailrec def collect(acc: ListBuffer[() => Int]): ListBuffer[() => Int] = {
    acc += (() => v)
    if (next == null) acc else next.nn.collect(acc)
  }
  @tailrec def viaArg(n: Int, fs: List[() => Int]): List[() => Int] =
    if (n == 0) fs else viaArg(n - 1, (() => n * 10 + v) :: fs)
  @tailrec def withClass(n: Int, out: ListBuffer[AnyRef]): ListBuffer[AnyRef] =
    if (n == 0) out
    else {
      out += new AnyRef { override def toString = s"k$n/$v" }
      withClass(n - 1, out)
    }
  @tailrec def withLocal(n: Int, out: ListBuffer[Int]): ListBuffer[Int] = {
    def twice = n * 2 + v
    out += twice
    if (n == 0) out else withLocal(n - 1, out)
  }
}

object tailcaptures {
  @tailrec def loop(n: Int, fs: List[() => Int]): List[() => Int] =
    if (n == 0) fs else { val f = () => n; loop(n - 1, f :: fs) }

  @tailrec def mixed(n: Int, total: Int, fs: List[() => Int]): (Int, List[() => Int]) = {
    var local = n
    val g = () => { local += 1; local + total }
    if (n == 0) (total, g :: fs) else mixed(n - 1, total + g(), g :: fs)
  }

  def nested(k: Int): List[Int] = {
    @tailrec def go(i: Int, acc: List[() => Int]): List[() => Int] =
      if (i == 0) acc else go(i - 1, (() => i + k) :: acc)
    go(3, Nil).map(_())
  }

  def main(args: Array[String]): Unit = {
    println(loop(4, Nil).map(_()))
    val n = new Node(1, new Node(2, new Node(3, null)))
    println(n.collect(ListBuffer()).map(_()).toList)
    println(n.viaArg(3, Nil).map(_()))
    println(n.withClass(3, ListBuffer()).toList)
    println(n.withLocal(3, ListBuffer()).toList)
    val (t, gs) = mixed(3, 0, Nil)
    println(t + " " + gs.map(_()))
    println(nested(100))
  }
}
