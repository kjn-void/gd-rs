package indexsort

import kotlin.random.Random

/** Row count in the C++ benchmark. */
const val ROW_COUNT = 10_000

/** Generated values for one row. C6 is a `uint32` stored in an `Int`. */
class SourceRow(
    val c1: Long,
    val c2: String,
    val c3: Double,
    val c4: Double,
    val c5: Int,
    val c6: Int,
    val c7: String,
)

/**
 * Same distributions and draw order as the C++ test, with every column set: C4 a
 * double like C3, and C5 and C6 a draw like C1 modulo 1000. The engine differs, so the
 * values do too.
 */
fun generateRows(count: Int): List<SourceRow> {
    val random = Random(42)
    fun letters(length: Int): String =
        String(CharArray(length) { 'a' + random.nextInt(26) })

    return List(count) {
        val c2 = letters(8)
        val c7 = "01234567890123456789" + letters(25)
        val c1 = random.nextLong(0, 1_000_001)
        val c3 = random.nextDouble(0.0, 1_000.0)
        val c4 = random.nextDouble(0.0, 1_000.0)
        val c5 = (random.nextLong(0, 1_000_001) % 1000).toInt()
        val c6 = (random.nextLong(0, 1_000_001) % 1000).toInt()
        SourceRow(c1, c2, c3, c4, c5, c6, c7)
    }
}
