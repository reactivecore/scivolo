import scala.collection.mutable

final class Deferrer(private val l: () => Int) extends AnyVal {
  def get: Int = loweringshapes.g(l())
}

trait Outer {
  val Info: InfoModule
  trait InfoModule { this: Info.type =>
    def name: String
  }
}
class Chan {
  private final val Signal = ()
  private val nul: Null = null
  def write(b: mutable.ArrayBuffer[Unit]): Unit = {
    b += Signal
    println(nul)
  }
}

trait Ops[A] {
  def items: List[A]
  def first: Int = items.map(x => x.hashCode).sum

  private class Itr {
    def init() = {
      val m = mutable.HashMap[A, Int]()
      items.map(e => (e, m.getOrElseUpdate(e, m.size)))
    }
  }

  def itr: Any = new Itr().init()

  def diff(that: List[A]): List[A] = {
    val occ = mutable.HashMap[A, Int]()
    that.foreach(y => occ.updateWith(y) { case None => Some(1); case Some(n) => Some(n + 1) })
    items.filter { x =>
      var include = false
      occ.updateWith(x) {
        case None => include = true; None
        case Some(n) => Some(n - 1)
      }
      include
    }
  }
}

object loweringshapes extends Ops[Int] {
  def items = List(1, 2, 2, 3)
  def g(y: => Int): Int = y + 1
  def fwd(x: => Int): Int = g(x)
  def pairs(t: (Int, (String, Int))): String = t match {
    case (a, (b, c)) => s"$b${a + c}"
  }
  def main(args: Array[String]): Unit = {
    println(fwd(41))
    println(new Deferrer(() => 1).get)
    val b = mutable.ArrayBuffer[Unit]()
    new Chan().write(b)
    println(b)
    println(first)
    println(itr)
    println(diff(List(2, 5)))
    println(pairs((1, ("x", 2))))
  }
}
