//> using options -Yexplicit-nulls --fresh
object CaseNull:
  def a(n: String | Null) = n match
    case null => "none"
    case s => s
  def b(n: String | Null) = n match
    case null => 0
    case s: String => s.length
  def c(n: String | Null) = n match
    case "x" => n
    case _ => "y"
  def d(n: String | Null): Int = n match
    case null => 0
    case s => s.length
  def e(n: Option[String] | Null) = n match
    case null => None
    case Some(s) => Some(s)
    case o => o
  def p(n: String | Null) = n match
    case null | "" => 0
    case s => s.length
