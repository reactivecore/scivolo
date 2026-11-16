package scala.probe

class tag(x: Int) extends scala.annotation.StaticAnnotation
class mark extends scala.annotation.StaticAnnotation

trait GenericSizes[A] {
  def size(that: List[A] @tag(1)): Int = that.length
  def sizeOf[B](that: List[B] @tag(2)): Int = that.length
  def both(a: List[A] @tag(1), b: List[A] @tag(2)): Int @tag(3) = 0
  def marks(a: List[A @mark] @mark): Int = 0
  def twice(a: List[Int] @tag(1) @tag(2)): Int = 0
}

trait GenericLengths[A] extends GenericSizes[A] {
  override def size(that: List[A] @tag(1)): Int = 0
  override def sizeOf[B](that: List[B] @tag(2)): Int = 0
  override def both(a: List[A] @tag(1), b: List[A] @tag(2)): Int @tag(3) = 1
  override def marks(a: List[A @mark] @mark): Int = 1
  def len(that: List[A] @tag(1)): Int = super.size(that)
  def lenOf[B](that: List[B] @tag(2)): Int = super.sizeOf(that)
  def lenBoth(a: List[A] @tag(1), b: List[A] @tag(2)): Int = super.both(a, b)
  def lenMarks(a: List[A @mark] @mark): Int = super.marks(a)
  override def twice(a: List[Int] @tag(1) @tag(2)): Int = 1
  def lenTwice(a: List[Int] @tag(1) @tag(2)): Int = super.twice(a)
}
