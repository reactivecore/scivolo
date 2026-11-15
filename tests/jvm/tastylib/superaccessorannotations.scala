package scala.probe

trait Sizes {
  def sizeCompare(that: List[Any] @unchecked): Int = that.length
}

trait Lengths extends Sizes {
  override def sizeCompare(that: List[Any] @unchecked): Int = 0
  def lengthCompare(that: List[Any] @unchecked): Int = super.sizeCompare(that)
}
