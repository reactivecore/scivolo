class Revealing extends mixins.Secret

object SecretMain {
  def main(args: Array[String]): Unit = println(new Revealing().reveal)
}
