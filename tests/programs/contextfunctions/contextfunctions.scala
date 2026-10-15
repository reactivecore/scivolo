// Context functions: terms wrapped in context closures, `?=>` types, implicit and explicit
// applications of context function values, methods with context function results.

class Config(val name: String, val level: Int)

object Contexts {
  type Configured[T] = Config ?=> T

  def run[T](c: Config)(body: Configured[T]): T = body(using c)

  def name(using c: Config): String = c.name

  val greeting: Configured[String] = "hello " + name

  val describe: (String, Config) ?=> String = summon[String] + ": " + name + "/" + summon[Config].level

  val repeat: (Int, Config) ?=> String = (n: Int, c: Config) ?=> c.name * n

  val curried: Int ?=> Config ?=> String = name * summon[Int]

  def explicitly(c: Config): Int = {
    val f: Config ?=> Int = name.length + 1
    f(using c) + f.apply(using c)
  }

  def nested(c: Config): String = run(c) {
    val inner = run(new Config("inner", 2))(name)
    inner + "/" + name
  }

  def twice[A](f: A ?=> A)(a: A): A = f(using f(using a))

  def greet: Configured[String] = "hi " + name

  def tagged[A](n: Int)(a: A): Int ?=> Config ?=> String = name * n + a + summon[Int]

  def local(c: Config): String = {
    def shout: Configured[String] = name.toUpperCase + "!"
    shout(using c) + greet(using c)
  }
}

object contextfunctions {
  import Contexts.*

  def main(args: Array[String]): Unit = {
    val c = new Config("cfg", 1)
    println(run(c)(name.length))
    println(run(c)(greeting))
    println(describe(using "d", c))
    println(repeat(using 3, c))
    println(run(c)(curried(using 2)))
    println(explicitly(c))
    println(nested(c))
    println(twice(summon[Int] * 3)(2))
    given Config = new Config("given", 3)
    println(greeting)
    println(curried(using 1))
    println(greet)
    println(tagged(2)('x')(using 7))
    println(local(c))
  }
}
