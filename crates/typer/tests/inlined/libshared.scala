//> using options --fresh -classpath crates/typer/tests/inlined/lib/lib.jar
import inl.Shared

object Test {
  def main(args: Array[String]): Unit = {
    println(Shared.withThis(args.length))
    println(Shared.exported(args.length) + Shared.exportedToo(2))
  }
}
