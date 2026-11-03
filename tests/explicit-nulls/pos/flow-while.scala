//> using options -Yexplicit-nulls --fresh
object FlowWhile:
  class C(val x: Int, val next: C | Null)

  def list(): Int =
    var xs: C | Null = C(1, C(2, null))
    var sum = 0
    while xs != null do
      val xsx: Int = xs.x
      val xscpy: C = xs
      if xscpy.next != null then sum += xscpy.next.x
      sum += xsx
      xs = xs.next
    sum

  def after(s: String | Null): Int =
    var x: String | Null = s
    while x == null do x = "a"
    x.length

  def untouched(s: String | Null): Int =
    var x: String | Null = s
    var n = 0
    if x != null then
      while n < 3 do n += 1
      n + x.length
    else n

  def retracted(s: String | Null): Int =
    var x: String | Null = s
    var n = 0
    if x != null then
      while n < 3 do
        if x != null then n += x.length
        x = s
    n

  def keep(s: String | Null): Int = 1

  def blockCondition(s: String | Null): Int =
    var x: String | Null = s
    var n = 0
    while { n += 1; x != null } do
      n += keep(x)
      x = if n > 3 then null else "a"
    n

  def stable(s: String | Null): Int =
    var n = 0
    while s != null && n < s.length do n += 1
    n
