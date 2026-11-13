trait Box:
  type E
  def get: E

object DependentLowerBounds:
  def dep(b: Box): b.E = b.get
  val bx: Box { type E = Int } = ???
  def mk(): Box { type E = String } = ???
  def i[A](a: A): A = a

  def o = Option(dep(bx))
  def l = List(dep(bx), dep(bx))
  def j = i(dep(bx))
  def k: Option[Int] = Option(dep(bx))
  def s = Option(dep(mk()))
  def p = (dep(bx), 1)
