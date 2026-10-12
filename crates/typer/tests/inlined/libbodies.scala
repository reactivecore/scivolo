//> using options --fresh -classpath crates/typer/tests/inlined/lib/lib.jar
import inl.Lib

object Test {
  def main(args: Array[String]): Unit = {
    val n = args.length
    println(Lib.kind(n))
    println(Lib.orElse(Some(n)))
    println(Lib.safeDiv(n))
    if n > 3 then Lib.fail("many")
    println(Lib.local(n))
    println(Lib.adder(n)(2))
    println(Lib.pair(n))
    println(Lib.viaTransparent(n))
  }
}
