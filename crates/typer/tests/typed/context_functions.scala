import scala.util.boundary, boundary.break

class Ctx(val name: String)
object ContextFunctions {
  type In[T] = Ctx ?=> T
  def run[T](c: Ctx)(body: In[T]): T = body(using c)
  def name(using c: Ctx): String = c.name
  def greeting: In[String] = "hello " + name
  val pair: (Int, Ctx) ?=> String = (n: Int, c: Ctx) ?=> c.name * n
  def curried: Int ?=> Ctx ?=> String = name * summon[Int]
  def both[A, B]: A ?=> B ?=> (A, B) = (summon[A], summon[B])
  def a(c: Ctx) = run(c)(greeting)
  def b(c: Ctx) = run(c)(name.length)
  def c(c: Ctx) = pair(using 2, c) + pair.apply(using 1, c)
  def d(c: Ctx) = curried(using 1)(using c)
  def e(c: Ctx) = both(using 1)(using c)
  def f(c: Ctx) = run(c) { (x: Ctx) ?=> x.name.length }
  def g(c: Ctx) = List[In[Int]](name.length).map(f => f(using c))
  def h(x: Int): Int =
    boundary:
      if x < 0 then break(0)
      x
}
