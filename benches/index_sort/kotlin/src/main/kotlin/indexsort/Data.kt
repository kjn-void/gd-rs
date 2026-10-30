package indexsort

import kotlin.random.Random

/** Row count in the C++ benchmark. */
const val ROW_COUNT = 1000

const val MILLION_ROWS = 1_000_000

/** Generated values for one row; C4 to C6 are never set, like the C++ test. */
class SourceRow(val c1: Long, val c2: String, val c3: Double, val c7: String)

/**
 * Same distributions and draw order as the C++ test. The engine differs, so the
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
        SourceRow(c1, c2, c3, c7)
    }
}
