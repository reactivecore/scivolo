trait LSeq[+A] extends LOps[A, LSeq, LSeq[A]]

trait LOps[+A, +CC[X] <: LSeq[X], +C <: LSeq[A] & LOps[A, CC, C]] {
  def tail: C
  def coll: C
  def length: Int = {
    var these = coll
    var len = 0
    while (len < 3) {
      len += 1
      these = these.tail
    }
    len
  }
}
