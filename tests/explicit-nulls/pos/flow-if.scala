//> using options -Yexplicit-nulls --fresh
object FlowIf:
  def apply[A](x: A | Null): Option[A] = if (x == null) None else Some(x)
  def len(s: String | Null): Int = if s != null then s.length else 0
  def len2(s: String | Null): Int = if null != s then s.length else -1
  def both(a: String | Null, b: String | Null): Int =
    if a != null && b != null then a.length + b.length else 0
  def either(a: String | Null): Int = if a == null || a.length == 0 then 0 else a.length
  def not(a: String | Null): Int = if !(a == null) then a.length else 0
  def eqs(a: AnyRef | Null): AnyRef = if a ne null then a else "x"
  def eqs2(a: AnyRef | Null): AnyRef = if a eq null then "y" else a
  def and(s: String | Null): Boolean = s != null && s.length > 1
  def or(s: String | Null): Boolean = s == null || s.isEmpty
  def nested(a: String | Null, b: String | Null): Int =
    if a != null then
      if b != null then a.length + b.length else a.length
    else 0
  def loop(s: String | Null): Int =
    var n = 0
    while s != null && n < s.length do n += 1
    n
  final class Box(val v: String | Null)
  def field(b: Box): Int = if b.v != null then b.v.length else 0
