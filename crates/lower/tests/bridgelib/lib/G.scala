package fix

trait G[R] {
  protected def flatten(vs: R*): R
  def pair(a: R, b: R): R = flatten(a, b)
}
