class Base:
  def name: String = "base"
  def size(k: Int): Int = k

trait Named:
  def name: String = "named"

object Lib:
  inline def counter(n: Int): Int =
    object Acc:
      var total = 0
      def add(k: Int): Unit = total += k
    Acc.add(n)
    Acc.add(n + 1)
    Acc.total
  inline def derived(s: String): Base = new Base:
    override def name = super.name + s
    override def size(k: Int) = super.size(k) * 2
  inline def mixed(s: String): Base = new Base with Named:
    override def name = super[Named].name + super[Base].name + s

object Other:
  def go(): Int = Lib.counter(10)

@main def Lw26objects(): Unit =
  println(Lib.counter(3))
  println(Other.go())
  val d = Lib.derived("!")
  println(d.name)
  println(d.size(4))
  println(Lib.mixed("?").name)
  println(Lib.counter(1) + Lib.counter(2))
