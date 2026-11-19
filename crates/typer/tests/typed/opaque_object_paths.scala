object o { u =>
  opaque type T = String

  def length(x: o.T): Int = x.length
  def upper(x: u.T): o.T = x.toUpperCase
  def wrap(s: String): o.T = s
  def id(x: o.T): o.T = x
  def same = summon[String =:= o.T]
  inline def unwrapped: String = id("a")
}

object p:
  opaque type Counter = Int
  def next(n: Int): Counter = q.g(n) + 1

object q:
  def g(n: p.Counter): p.Counter = n
