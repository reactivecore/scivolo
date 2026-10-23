trait TDTrait:
  def t: Int
  def u: Int

trait TDManaged(x: Int):
  def xx = x
  def flatMap(f: Int => TDManaged): TDManaged = new TDManaged(x + 1):
    def make() = new TDTrait:
      val t = x
      val u =
        def foo = f(t)
        foo.xx
    override def xx = make().u

object TDanonnested:
  def main(args: Array[String]): Unit =
    val m = new TDManaged(20) {}
    println(m.flatMap(y => new TDManaged(y * 2) {}).xx)
