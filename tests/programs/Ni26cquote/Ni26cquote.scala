import scala.scalanative.unsafe.*
import scala.scalanative.libc.string.strlen

object Ni26cquote {
  def main(args: Array[String]): Unit = {
    val hello: CString = c"hello"
    println(fromCString(hello))
    println(strlen(c"four"))
    println(fromCString(c"tab\there\nnew line"))
    println(fromCString(c"quote \" and \\ and \x41\x42"))
    println(strlen(c""))
    println(fromCString(c"ünï"))
  }
}
