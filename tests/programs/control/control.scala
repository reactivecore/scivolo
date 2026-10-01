//> using scala 3.9.0
object Flow:
  var hits = 0

  def pick(c: Boolean): String = if c then "yes" else "no"

  def side(c: Boolean): Unit = if c then println("side")

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
  println(Flow.count(4))
  println(Flow.early(true))
  println(Flow.early(false))
  println(Flow.wide())
  Flow.bump()
  println(Flow.hits)
