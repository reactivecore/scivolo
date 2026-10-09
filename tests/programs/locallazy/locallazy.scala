//> using scala 3.9.0
object Lazies:
  def twice(n: Int): Int =
    println("start")
    lazy val x: Int =
      println("computing x")
      n * 2
    x + x

  def strings(n: Int, s: String): String =
    lazy val a = s * n
    lazy val b = a + "!"
    val f = () => b + a
    f()

  def squares(xs: List[Int]): List[Long] =
    xs.map { i =>
      lazy val sq: Long = i.toLong * i
      sq + sq
    }

  def unused(): String =
    lazy val boom: String = throw IllegalStateException("never")
    "unused"

  def retried(): String =
    var attempts = 0
    lazy val flaky: String =
      attempts += 1
      if attempts < 2 then throw IllegalStateException("not yet")
      s"ok after $attempts"
    val first = try flaky catch case e: IllegalStateException => e.getMessage
    first + ", " + flaky

  def generic[T](t: T): T =
    lazy val g: T = t
    g

  def nested(): Double =
    lazy val d: Double =
      lazy val d: Double = 1.5
      d * 2
    d

  def unit(): String =
    lazy val u: Unit = println("unit once")
    val r1: Unit = u
    val r2: Unit = u
    "units"

class Shadow(k: Int):
  lazy val x: Int = k + 1
  def shadow(): Int =
    lazy val x: Int = k + 2
    x + this.x
  def bound(o: Option[Int]): Int =
    lazy val v: Int = o match
      case Some(y) => y
      case None => 0
    v
  def innerVar(): Int =
    lazy val r: Int =
      var n = 0
      val f = () => n += 1
      f(); f()
      n
    r

class Box(n: Int):
  lazy val field: Int = n + 1
  def get(): Int =
    lazy val local: Int = field * n
    local

object Locks:
  val lock = new Object
  def count(n: Int): Int = lock.synchronized { n + 1 }
  def say(s: String): Unit = this.synchronized { println(s) }

@main def locallazy(): Unit =
  println(Lazies.twice(21))
  println(Lazies.strings(2, "ab"))
  println(Lazies.squares(List(1, 2, 3)))
  println(Lazies.unused())
  println(Lazies.retried())
  println(Lazies.generic("g"))
  println(Lazies.nested())
  println(Lazies.unit())
  val shadow = Shadow(1)
  println(s"${shadow.shadow()} ${shadow.bound(Some(5))} ${shadow.bound(None)} ${shadow.innerVar()}")
  println(Box(3).get())
  println(Locks.count(1))
  Locks.say("synchronized")
  lazy val c: Char = 'c'
  lazy val z: Boolean = true
  lazy val by: Byte = 1.toByte
  lazy val sh: Short = 2.toShort
  lazy val fl: Float = 3f
  println(s"$c $z $by $sh $fl")
