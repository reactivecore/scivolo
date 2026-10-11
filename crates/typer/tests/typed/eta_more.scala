class C:
  def cls = this.getClass
  def add(x: Int): Int = x + 1
  def sum(xs: Int*): Int = xs.sum
  def pair(a: Int, xs: Int*)(b: Int): Int = a + b + xs.length

object O:
  def mk(): C = new C
  def id[T](x: T): T = x
  def next(): Int = 1
  val add = mk().add
  val f = id
  val g: Int => Int = id
  val h = id _
  val x: C = new C
  val k = x.getClass
  val p = mk().pair(next(), next(), next())
  val q = mk().pair(1, Seq(next())*)
