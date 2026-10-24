trait Ctx:
  type T
  val x: T

object Ty24Dep:
  val f: (x: Int) => Int = x => x + 1
  val g: (c: Ctx) => c.T = c => c.x
  def h(c: Ctx): c.T = g(c)
  val k: (c: Ctx) ?=> c.T = summon[Ctx].x
