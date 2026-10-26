class TC(val n: Int)

class TV(val tc: TC) extends AnyVal:
  def twice: Int = tc.n * 2

class IV(val i: Int) extends AnyVal

def mk(n: Int): TV = new TV(new TC(n))

@main def Lw26lazyvc(): Unit =
  lazy val t: TV =
    println("init")
    new TV(new TC(3))
  println(t.twice)
  println(t.tc.n)
  lazy val i = new IV(5)
  println(i.i + 1)
  given g: TV = new TV(new TC(4))
  println(summon[TV].twice)
  val flag = i.i > 2
  lazy val c = if flag then mk(6) else new TV(new TC(7))
  println(c.twice)
  lazy val m = mk(8)
  val f = () => m.twice + i.i
  println(f())
  val boxed: Any = m
  println(boxed.isInstanceOf[TV])
