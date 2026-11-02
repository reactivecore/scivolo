//> using options -Yexplicit-nulls --fresh
object NullBasics:
  val s: String | Null = null
  val t: String = "a"
  val u: String | Null = t
  val a: Any = null
  val m: Matchable = null
  val n: Null = null
  def nn(x: String | Null): String = x.nn
  def id[T](x: T | Null): T | Null = x
  val i = id[String](null)
  val v = if t.length > 0 then t else null
  val w = if t.length > 0 then null else 1
  val l = List(t, null)
  val o: Option[String | Null] = Some(null)
  val arr: Array[String | Null] = Array(null, t)
  def isNull(x: AnyRef | Null): Boolean = x eq null
  def eqNull(x: String | Null): Boolean = x == null
  def pick(x: Int): String | Null = x match
    case 0 => null
    case _ => "x"
  var r: String | Null = null
  r = t
  val len = nn(s).length
  def bounded[T >: Null <: AnyRef | Null](x: T): T = x
  val b = bounded[String | Null](null)
  def pair(x: Int) = (if x > 0 then "a" else null, if x > 0 then List(1) else null)
