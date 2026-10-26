import scala.scalanative.unsafe.*
import scala.scalanative.unsigned.*
import scala.scalanative.posix.dlfcn.*

object Ni26funcptr {
  def lookup(name: CString): CVoidPtr = dlsym(dlopen(null, RTLD_LAZY), name)

  def main(args: Array[String]): Unit = {
    val abs = CFuncPtr.fromPtr[CFuncPtr1[CInt, CInt]](lookup(c"abs"))
    println(abs(-42))
    val labs = CFuncPtr.fromPtr[CFuncPtr1[CLongLong, CLongLong]](lookup(c"llabs"))
    println(labs(-7L))
    val strlen = CFuncPtr.fromPtr[CFuncPtr1[CString, CSize]](lookup(c"strlen"))
    println(strlen(c"hello"))
    val atoi = CFuncPtr.fromPtr[CFuncPtr1[CString, CInt]](lookup(c"atoi"))
    val n: Int = atoi(c"123")
    println(n + 1)
    val rand = CFuncPtr.fromPtr[CFuncPtr0[CInt]](lookup(c"rand"))
    println(rand() >= 0)
    val strncmp = CFuncPtr.fromPtr[CFuncPtr3[CString, CString, CSize, CInt]](lookup(c"strncmp"))
    println(strncmp(c"abc", c"abd", 2.toUSize))
  }
}
