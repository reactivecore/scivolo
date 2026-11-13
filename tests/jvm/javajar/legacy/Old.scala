package legacy

class Old(val n: Int) { def twice: Int = n * 2 }

class StringBuilder(n: Int) { override def toString = "legacy" }
