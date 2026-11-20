object TypeUtils:
  extension (t: String) def isByName: Boolean = t.startsWith("=>")

object Util:
  opaque type Arg = Int | String
  def arg(x: Int | String): Arg = x
  extension (arg: Arg) def isByName: Boolean = arg.isInstanceOf[String]

import TypeUtils.*
import Util.*

@main def ambiguousextensions = println((arg(1).isByName, arg("s").isByName))
