package indexsort

import kotlin.random.Random

/** Row count in the C++ benchmark. */
const val ROW_COUNT = 1000

const val MILLION_ROWS = 1_000_000

/** Row count in the C++ three-sort-method benchmark. */
const val SORT_METHODS_ROW_COUNT = 10_000

/**
 * Generated values for one row. The six-case benchmark never sets C4 to C6. C6 is a
 * `uint32` stored in an `Int`.
 */
class SourceRow(
    val c1: Long,
    val c2: String,
    val c3: Double,
    val c7: String,
    val c4: Double? = null,
    val c5: Int? = null,
    val c6: Int? = null,
)

/**
 * Rows for the six-case benchmark, with C4 to C6 null. Same distributions and draw
 * order as the C++ test. The engine differs, so the values do too.
 */
fun generateRows(count: Int): List<SourceRow> = generate(count, allColumns = false)

/**
 * Rows for the three-sort-method benchmark, with every column set: C4 a double like
 * C3, and C5 and C6 a draw like C1 modulo 1000.
 */
fun generateAllColumns(count: Int): List<SourceRow> = generate(count, allColumns = true)

private fun generate(count: Int, allColumns: Boolean): List<SourceRow> {
    val random = Random(42)
    fun letters(length: Int): String =
        String(CharArray(length) { 'a' + random.nextInt(26) })

    return List(count) {
        val c2 = letters(8)
        val c7 = "01234567890123456789" + letters(25)
        val c1 = random.nextLong(0, 1_000_001)
        val c3 = random.nextDouble(0.0, 1_000.0)
        if (allColumns) {
            val c4 = random.nextDouble(0.0, 1_000.0)
            val c5 = (random.nextLong(0, 1_000_001) % 1000).toInt()
            val c6 = (random.nextLong(0, 1_000_001) % 1000).toInt()
            SourceRow(c1, c2, c3, c7, c4, c5, c6)
        } else {
            SourceRow(c1, c2, c3, c7)
        }
    }
}
