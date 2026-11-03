//> using options -Yexplicit-nulls --fresh
object FlowTry:
  def assigns(): String =
    var x: String | Null = "foo"
    try x = x
    catch case e => x = x
    finally x = x
    if x == null then "foo" else x

  def body(s: String | Null): Int =
    var x: String | Null = s
    try
      x = "a"
      x.length
    catch case _: Exception => 0

  def after(s: String | Null): Int =
    var x: String | Null = s
    try x = "a"
    catch case _: Exception => x = "b"
    x.length

  def stable(s: String | Null): Int =
    try
      if s == null then 0 else s.length
    finally ()

  def handler(s: String | Null): Int =
    val n =
      try 1
      catch case e: Exception => if s != null then s.length else 0
    n
