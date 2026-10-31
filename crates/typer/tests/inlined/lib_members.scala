//> using options --fresh -classpath crates/typer/tests/inlined/lib/lib.jar
import inl.{Greeter, Greeters, Overloads}

object LibMembers:
  val partial = Overloads.andThen(1) { case x if x % 2 == 0 => x }
  val total = Overloads.andThen(1)((x: Int) => x + 1)

class Exporting extends Greeter:
  private val delegate = Greeters.hello
  export delegate._
