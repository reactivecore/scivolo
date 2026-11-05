class B[A](n: Int) { var size0 = 0 }
final class V[A](u: B[A], f: () => Int) {
  def size = u.size0 + f()
  def this(len: Int) = {
    this({
      val l = len
      val b = new B[A](0) {
        this.size0 = l
      }
      b
    }, () => 0)
  }
}
