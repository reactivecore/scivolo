package pp {
  trait Parser { def decode(s: String): Int = s.length }
  package object parser extends Parser
}
package qq {
  import pp.parser.decode
  object PackageObjectInherited {
    val x = decode("ab")
    val y = pp.parser.decode("a")
  }
}
