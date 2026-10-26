import scala.scalanative.meta.LinktimeInfo
import scala.scalanative.unsafe.resolvedAtLinktime

object Ni26linktime {
  @resolvedAtLinktime
  def isUnixLike: Boolean = LinktimeInfo.isLinux || LinktimeInfo.isMac

  @resolvedAtLinktime
  def pointerBits: Int = if (LinktimeInfo.is32BitPlatform) 32 else 64

  def os(): String =
    if (LinktimeInfo.isWindows) "windows"
    else if (LinktimeInfo.isLinux) "linux"
    else if (LinktimeInfo.isMac) "mac"
    else "other"

  def checks(): Unit = {
    if (!LinktimeInfo.isWindows) println("not windows")
    if (LinktimeInfo.isLinux || LinktimeInfo.isMac) println("unix")
    if ((LinktimeInfo.isMac || LinktimeInfo.isLinux) && !LinktimeInfo.is32BitPlatform) println("64-bit unix")
    if (LinktimeInfo.target.arch == "nonexistent") println("odd") else println("arch known")
    if (LinktimeInfo.target.arch != "nonexistent") println("arch not odd")
    if (isUnixLike) println("unix-like")
    if (pointerBits == 64) println("64 bits")
    if (pointerBits != 16) println("not 16 bits")
  }

  def main(args: Array[String]): Unit = {
    println(os() != "")
    checks()
    println(LinktimeInfo.isWindows)
  }
}
