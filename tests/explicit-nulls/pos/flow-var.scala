//> using options -Yexplicit-nulls --fresh
object FlowVar:
  def nullable[T](x: T): T | Null = x

  def assigned(): Int =
    var x: String | Null = nullable("a")
    x = ""
    x.length

  def tested(): Int =
    var x: String | Null = nullable("a")
    if x != null then
      val a: String = x
      x = ""
      val b: String = x
      a.length + b.length
    else 0

  def reassigned(): String =
    var x: String | Null = nullable("a")
    if x != null then
      val a: String = x
      x = nullable(a)
      if x == null then a else x
    else ""

  def ascribed(): Int =
    var x: String | Null = nullable("a")
    if x != null then
      val b: String | String = x
      x = b
      x.length
    else 0

  def early(s: String | Null): Int =
    var x: String | Null = s
    if x == null then return 0
    x.length

  def captured(): Int =
    var x: String | Null = nullable("a")
    val f = () => { x = null }
    f()
    if x != null then 1 else 0

  def nested(): Int =
    var x: String | Null = nullable("a")
    def g: Int = if x != null then 1 else 0
    if x != null then x.length + g else g

  def lambdaUse(): Int =
    var x: String | Null = nullable("a")
    if x != null then
      val f = (n: Int) => n + 1
      f(x.length)
    else 0

  def inBlock(): Int =
    var x: String | Null = null
    val n = {
      x = "abc"
      1
    }
    if x != null then
      val y: String | Null = x
      n + 1
    else n

  def compound(): Int =
    var x: String | Null = "a"
    var n = 0
    n += 1
    if x != null then x.length + n else n

  def byName(n: => Int): Int = n
  def byNamePoly[A](a: => A): A = a
  def keep(s: String | Null): Int = 1

  def byNameArg(s: String | Null): Int =
    var y: String | Null = s
    if y != null then
      byName {
        var x: String | Null = nullable("a")
        x = "b"
        val f = () => { var z: String | Null = s; z = "c"; z.length }
        keep(x) + keep(y) + f()
      }
    else 0

  def byNamePolyArg(s: String | Null): Int =
    byNamePoly {
      var x: String | Null = s
      x = "b"
      x.length
    }

  class Holder:
    var field: String | Null = null
    def len: Int = if field != null then 1 else 0
