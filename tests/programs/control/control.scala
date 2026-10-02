//> using scala 3.9.0
object Flow:
  var hits = 0

  def pick(c: Boolean): String = if c then "yes" else "no"

  def side(c: Boolean): Unit = if c then println("side")

  def unitAny(c: Boolean): Any = if c then 5

  def strAny(c: Boolean): Any = if c then "a" else "b"

  var going = true
  var total = 0

  def count(n: Int): Int =
    while going do
      total = n
      going = false
    total

  def early(c: Boolean): String =
    if c then return "early"
    "late"

  def wide(): Long = 3

  def bump(): Unit = hits = 5

@main def control(): Unit =
  println(Flow.pick(true))
  println(Flow.pick(false))
  Flow.side(true)
  println(Flow.unitAny(true))
  println(Flow.strAny(false))
  println(Flow.count(4))
  println(Flow.early(true))
  println(Flow.early(false))
  println(Flow.wide())
  Flow.bump()
  println(Flow.hits)
