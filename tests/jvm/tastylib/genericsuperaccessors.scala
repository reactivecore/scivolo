package scala.probe

class tag(x: Int) extends scala.annotation.StaticAnnotation

trait GenericSizes[A] {
  def size(that: List[A] @tag(1)): Int = that.length
  def sizeOf[B](that: List[B] @tag(2)): Int = that.length
}

trait GenericLengths[A] extends GenericSizes[A] {
  override def size(that: List[A] @tag(1)): Int = 0
  override def sizeOf[B](that: List[B] @tag(2)): Int = 0
  def len(that: List[A] @tag(1)): Int = super.size(that)
  def lenOf[B](that: List[B] @tag(2)): Int = super.sizeOf(that)
}
