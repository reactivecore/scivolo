//> using options -Yexplicit-nulls
object FlexibleWildcardCapture {
  def f[T](c: Class[T]): Class[T] = c
  def component(c: Class[?]) = f(c.getComponentType)
  def boxed(s: String) = f(s.getClass)
}
