//> using options -Yexplicit-nulls --fresh
object FlowBlock:
  def guard(s: String | Null): Int =
    if s == null then throw new IllegalArgumentException("null")
    s.length
  def ret(s: String | Null): Int =
    if s == null then return 0
    s.length
  def inner(s: String | Null): Int =
    val n =
      if s == null then throw new NullPointerException()
      s.length
    n + 1
  def nothing(s: String | Null): Int =
    if s == null then sys.error("null")
    s.length
  def localDefs(s: String | Null): Int =
    if s == null then throw new Exception()
    def h: Int = s.length
    val v = s.length
    h + v
  def nested(s: String | Null): Int =
    {
      if s == null then return 0
      ()
    }
    s.length
  def stats(a: String | Null, b: String | Null): Int =
    if a == null then throw new Exception()
    val x = a.length
    if b != null then x + b.length else x
  def lam(s: String | Null): () => Int =
    if s == null then throw new Exception()
    () => s.length
  def localVal(o: Object): Int =
    val s: String | Null = o.toString
    if s != null then s.length else 0
