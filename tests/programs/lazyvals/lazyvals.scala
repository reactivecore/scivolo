//> using scala 3.9.0
object Config:
  var loads = 0
  lazy val answer: Int =
    loads += 1
    41
  lazy val label: String = "answer=" + answer
  lazy val maybe: String = null

class Counter(start: Int):
  lazy val doubled: Int =
    println(s"computing $start")
    start * 2
  private lazy val hidden: Long = doubled.toLong * 1000
  def show = s"$doubled/$hidden"

class Flaky:
  var attempts = 0
  lazy val value: String =
    attempts += 1
    if attempts < 2 then throw IllegalStateException("not yet")
    s"ok after $attempts"

object Flaky:
  def make() = Flaky()

@main def lazyvals(): Unit =
  println(Config.loads)
  println(Config.answer + 1)
  println(Config.answer)
  println(Config.label)
  println(Config.maybe == null)
  println(Config.loads)
  val c = Counter(4)
  println("created")
  println(c.doubled)
  println(c.show)
  val f = Flaky.make()
  try println(f.value)
  catch case e: IllegalStateException => println(e.getMessage)
  println(f.value)
  println(f.value)
