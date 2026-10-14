//> using scala 3.9.0
class Path(action: () => Unit, parent: Option[Path]):
  private var count = 0
  private def bump(): Int =
    count += 1
    count
  object O:
    def apply(): Unit =
      action()
      parent match
        case Some(p) => p.O.apply()
        case None =>
  private object Base:
    def at: Int = 10
  class Step:
    def next: Int = bump() + Base.at

class C(x: Int):
  override def toString(): String = "C" + v.value
  val v = new D
  class D:
    def value: Int = x

object Registry:
  private var n = 0
  private def twice(i: Int): Int = i * 2
  def count: Int = twice(n)
  object Inc:
    def apply(): Int =
      n += 1
      twice(n)

class Counter(private var n: Int):
  def next(): Int =
    n += 1
    n
  def same(o: Counter): Boolean = o.n == n

case class Cell[T](private var t: T):
  def get = t
  def set(t2: T) = t = t2

@main def privatemembers(): Unit =
  val counter = Counter(1)
  println(counter.next() + counter.next())
  println(counter.same(Counter(3)))
  val cell = Cell("a")
  cell.set("b")
  println(cell.get)
  println(Registry.Inc() + Registry.Inc() + Registry.count)
  val path = Path(() => println("act"), Some(Path(() => println("parent"), None)))
  path.O()
  val s = new path.Step
  println(s.next + s.next)
  println(new C(7))
