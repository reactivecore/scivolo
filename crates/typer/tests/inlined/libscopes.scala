//> using options --fresh -classpath crates/typer/tests/inlined/lib/lib.jar
import inl.Scopes

object Test {
  given inl.Show[Int] = a => "member " + a
  def main(args: Array[String]): Unit = {
    println(Scopes.scoped)
    println(Scopes.inClass(args.length + 1))
  }
}
