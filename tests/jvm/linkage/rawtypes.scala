import scala.scalanative.runtime.{RawPtr, RawSize}

object RawTypes {
  def isNull(p: RawPtr): Boolean = p == null
  def notNull(p: RawPtr): Boolean = null != p
  def same(p: RawPtr, q: RawPtr): Boolean = p == q
  def sameSize(a: RawSize, b: RawSize): Boolean = a == b
  def kept(p: RawPtr): Any = p
  def held(p: RawPtr): Array[RawPtr] = Array(p)
}
