//> using scala 3.9.0

import scala.scalanative.meta.LinktimeInfo
import scala.scalanative.unsafe.resolvedAtLinktime

/** `@resolvedAtLinktime` methods computed from link-time properties and from each other. */
object Platform:
  @resolvedAtLinktime def threads: Boolean = LinktimeInfo.isMultithreadingEnabled
  @resolvedAtLinktime def unixLike: Boolean = LinktimeInfo.isLinux || LinktimeInfo.isMac
  @resolvedAtLinktime def family: String = if unixLike then "unix" else "other"

@main def linktime(): Unit =
  println(Platform.family)
  println(if Platform.unixLike then "unix-like" else "other")
  val threads = if Platform.threads then "threads" else "no threads"
  println(threads.endsWith("threads"))
