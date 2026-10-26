import scala.scalanative.runtime.{Intrinsics, RawPtr, RawSize, fromRawPtr, toRawPtr}
import scala.scalanative.runtime.Intrinsics.*
import scala.scalanative.unsafe.*

class Cell {
  var count: Int = 7
}

object Ni26intrinsics {
  def loads(p: RawPtr): Unit = {
    storeInt(p, 42)
    println(loadInt(p))
    storeLong(p, -5L)
    println(loadLong(p))
    storeByte(p, 3.toByte)
    println(loadByte(p))
    storeShort(p, 9.toShort)
    println(loadShort(p))
    storeChar(p, 'x')
    println(loadChar(p))
    storeBoolean(p, true)
    println(loadBoolean(p))
    storeFloat(p, 1.5f)
    println(loadFloat(p))
    storeDouble(p, 2.25)
    println(loadDouble(p))
    storeRawPtr(p, p)
    println(loadRawPtr(p) == p)
    val q = elemRawPtr(p, 4)
    storeInt(q, 11)
    println(loadInt(elemRawPtr(p, castIntToRawSize(4))))
  }

  def casts(): Unit = {
    println(castIntToFloat(0x3f800000))
    println(castFloatToInt(1.0f))
    println(castLongToDouble(0x3ff0000000000000L))
    println(castDoubleToLong(1.0))
    val s = castIntToRawSize(-1)
    println(castRawSizeToInt(s))
    println(castRawSizeToLong(s))
    println(castRawSizeToLongUnsigned(castIntToRawSizeUnsigned(-1)))
    println(castRawSizeToLong(castLongToRawSize(12L)))
    val p = castLongToRawPtr(64L)
    println(castRawPtrToLong(p))
    println(castRawPtrToInt(castIntToRawPtr(32)))
    val o: Object = "obj"
    println(castRawPtrToObject(castObjectToRawPtr(o)))
  }

  def unsigned(): Unit = {
    println(divUInt(-1, 2))
    println(remUInt(-1, 7))
    println(divULong(-1L, 2L))
    println(remULong(-1L, 7L))
    val b: Byte = -1
    val s: Short = -1
    println(byteToUInt(b))
    println(byteToULong(b))
    println(shortToUInt(s))
    println(shortToULong(s))
    println(intToULong(-1))
    println(uintToFloat(-1))
    println(ulongToFloat(-1L))
    println(uintToDouble(-1))
    println(ulongToDouble(-1L))
    println(unsignedOf(-1))
    println(unsignedOf(-1L))
    println(unsignedOf(b))
    println(unsignedOf(s))
  }

  def field(): Unit = {
    val c = new Cell
    val p = classFieldRawPtr(c, "count")
    storeInt(p, 8)
    println(c.count)
  }

  def main(args: Array[String]): Unit = {
    loads(toRawPtr(scala.scalanative.libc.stdlib.malloc(64)))
    casts()
    unsigned()
    field()
  }
}
