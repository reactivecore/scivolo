trait Sized {
  def size: Int
  def twice(): Int = size * 2
}

trait Plus extends Sized {
  override def twice(): Int = super.twice() + 1
}

class Box(val size: Int) extends Plus

trait T1 {
  def m(): Unit = println("T1")
}

trait T2 extends T1

trait T3 extends T1 {
  override def m(): Unit = { println("T3"); super.m() }
}

class X extends T2 with T3 {
  override def m(): Unit = { super[T2].m(); super[T3].m(); super.m() }
}

class P extends T3

class Q extends P {
  override def m(): Unit = { println("Q"); super.m() }
}

object Traits {
  def main(args: Array[String]): Unit = {
    val b = new Box(5)
    println(b.twice())
    val s: Sized = b
    println(s.twice())
    println(s.size)
    new X().m()
    new Q().m()
  }
}
