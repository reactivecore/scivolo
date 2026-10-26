import scala.scalanative.unsafe.*
import scala.scalanative.unsigned.*
import scala.scalanative.runtime.Intrinsics

object Ni26stackalloc {
  def sizes(): Unit = {
    println(sizeOf[Int])
    println(sizeOf[Long])
    println(sizeOf[Byte])
    println(sizeOf[Ptr[Int]])
    println(sizeOf[CStruct2[Int, Long]])
    println(sizeOf[CArray[Int, Nat._4]])
    println(alignmentOf[Long])
    println(alignmentOf[CStruct3[Byte, Short, Int]])
    println(sizeof[Double])
    println(Intrinsics.castRawSizeToInt(Intrinsics.sizeOf[UInt]))
    println(Intrinsics.castRawSizeToInt(Intrinsics.alignmentOf[String]))
  }

  def stack(n: Int): Unit = {
    val one = stackalloc[Int]()
    !one = 5
    println(!one)
    val many = stackalloc[Long](4.toUSize)
    many(3) = 7L
    println(many(3))
    val raw = Intrinsics.stackalloc[Byte](Intrinsics.castIntToRawSize(n))
    Intrinsics.storeByte(raw, 1.toByte)
    println(Intrinsics.loadByte(raw))
    val sized = stackalloc[Int](n.toUSize)
    sized(1) = 9
    println(sized(1))
    val struct = stackalloc[CStruct2[Int, Double]]()
    struct._2 = 1.5
    println(struct._2)
  }

  def main(args: Array[String]): Unit = {
    sizes()
    stack(3)
  }
}
