class Thing {
  def info: Info[this.type] = Repository.getInfo(this)
  def info2: Info[this.type] = {
    def self: this.type = this
    Repository.getInfo(self)
  }
}

trait Info[T]
case class InfoImpl[T](thing: T) extends Info[T]

object Repository {
  def getInfo(t: Thing): Info[t.type] = InfoImpl(t)
}
