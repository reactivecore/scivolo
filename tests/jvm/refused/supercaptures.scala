import language.experimental.captureChecking

trait Base[+A]:
  def take(n: Int): Iterator[A]^{this} = Iterator.empty

trait Ops[+A] extends Base[A]:
  override def take(n: Int): Iterator[A]^{this} = super.take(n)
