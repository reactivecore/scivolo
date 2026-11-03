//> using options -Yexplicit-nulls --fresh
object FlowMatch:
  def lengthOfStringOrNull(s: String | Null): Int = s match
    case _: String => s.length
    case _ => 0

  def afterNull(s: String | Null): String = s match
    case null => "null"
    case _ => s

  def binds(s: String | Null): String = s match
    case null => "other"
    case s2 => s2

  def alternatives(s: String | Null): String = s match
    case null | "foo" => "other"
    case s2 => s2

  def typed(s: String | Null): String = s match
    case s3: String => s3
    case null => "other"
    case s4 => s4

  def anyRef(a: AnyRef | Null): String = a match
    case null => "null"
    case s: String => s
    case _ =>
      val a2: AnyRef = a
      a.toString

  def guarded(a: String | Null, b: String | Null): Int = a match
    case _ if b != null => b.length
    case _ => 0

  def assigns(): String =
    var x: String | Null = "foo"
    1 match
      case 1 => x = x
    if x == null then "foo" else x

  def joined(n: Int, s: String | Null): Int =
    var x: String | Null = s
    n match
      case 1 => x = "a"
      case _ => x = "bc"
    x.length

  def unapplied(o: Option[String] | Null): Int = o match
    case Some(v) => o.size + v.length
    case _ => 0
