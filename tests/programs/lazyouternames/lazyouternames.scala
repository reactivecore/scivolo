import scala.annotation.tailrec

class Channel(path: String, mode: String) {
  lazy val channel: String = mode + "!"
  def name: String = path
}

class Offsets(path: String) {
  lazy val offsets: Array[Int] =
    if (path.isEmpty()) Array(-1)
    else {
      var i = 0
      var count = 1
      while ({
        count += 1
        i = path.indexOf('/', i + 1)
        i != -1
      }) ()
      new Array[Int](count)
    }
}

class Stream {
  private class Builder {
    class Chunk(val n: Int)
    def chunk(n: Int): Chunk = new Chunk(n)
  }
  def size: Int = new Builder().chunk(3).n
}

trait Native {
  @volatile private var state = 0
  def bump(): Int = { state += 1; state }
}
class Worker extends Native

trait Show[T] { def show(t: T): String }
object Show {
  private object AnyShow extends Show[Any] { def show(t: Any): String = s"<$t>" }
  def any[T]: Show[T] = AnyShow.asInstanceOf[Show[T]]
}

class Glob {
  def build(specs: List[Int], next: Int): Int = {
    def loop(specs: List[Int], next: Int): Int = specs match {
      case h :: tail =>
        def plus(x: Int): Int = x + next
        loop(tail, plus(h) + plus(h + 1))
      case Nil => next
    }
    loop(specs, next)
  }
  @tailrec private def count(xs: List[Int], n: Int): Int = xs match {
    case _ :: tail => count(tail, n + 1)
    case Nil       => n
  }
  def counted(next: Int): Int = {
    @tailrec def count(xs: List[Int], n: Int): Int = if (xs.isEmpty) n + next else count(xs.tail, n + 1)
    count(List(1, 2), 0) + this.count(List(3), 0)
  }
}

object Main {
  def main(args: Array[String]): Unit = {
    val c = new Channel("p", "rw")
    println(c.channel + c.channel + c.name)
    println(new Offsets("/a/b").offsets.length)
    println(new Stream().size)
    val w = new Worker
    w.bump()
    println(w.bump())
    println(Show.any[Int].show(4))
    println(new Glob().build(List(1, 2), 0))
    println(new Glob().counted(5))
    println(java.util.ServiceLoader.load(classOf[Runnable]).iterator().hasNext())
  }
}
