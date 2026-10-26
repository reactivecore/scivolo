import scala.scalanative.runtime.LLVMIntrinsics.*

object Ni26struct {
  def add(a: Int, b: Int): String = {
    val overflow = `llvm.sadd.with.overflow.i32`(a, b)
    if (overflow.flag) "overflow" else overflow.value.toString
  }

  def mul(a: Long, b: Long): String = {
    val overflow = `llvm.umul.with.overflow.i64`(a, b)
    s"${overflow.value} ${overflow.flag}"
  }

  def main(args: Array[String]): Unit = {
    println(add(1, 2))
    println(add(Int.MaxValue, 1))
    println(mul(3L, 4L))
    println(mul(-1L, 2L))
  }
}
