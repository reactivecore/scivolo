//> using scala 3.9.0
// Enclosing instances referenced as bare `O.this`, and type patterns on a null scrutinee.

class K2 {
  val name = "k2"
  class L1 {
    def self = K2.this
    def pick(b: Boolean) = if (b) K2.this else null
    def arg = String.valueOf(K2.this)
    def last = { println("last"); K2.this }
    def qual = K2.this.name
  }
  override def toString = name
}

class K3 {
  class L2 {
    class L3 { def top = K3.this; def mid = L2.this }
  }
}

object Scrutinees {
  def plain(): String = (null: Any) match {
    case c: String => c
    case _ => "none"
  }
}

object Test {
  def run(): Unit = {
    val k = new K2
    val l = new k.L1
    println(l.self eq k)
    println(l.pick(false))
    println(l.pick(true))
    println(l.arg)
    println(l.last)
    println(l.qual)
    val k3 = new K3
    val l2 = new k3.L2
    val l3 = new l2.L3
    println(l3.top eq k3)
    println(l3.mid eq l2)
    println(Scrutinees.plain())
  }
}

@main def outerthis(): Unit = Test.run()
