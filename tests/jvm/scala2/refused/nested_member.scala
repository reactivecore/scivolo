import refused.Outer

extension (d: Outer.Deep.type) def same(x: Any): String = "extension"

object NestedMember {
  def main(args: Array[String]): Unit = println(Outer.Deep.same("member"))
}
