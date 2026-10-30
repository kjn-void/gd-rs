package indexsort

/**
 * Struct of arrays: one array per column, preallocated to `capacity` rows. C2 and C7
 * hold German strings; C4 to C6 are nullable and keep a validity array next to their
 * values.
 */
class Table(val capacity: Int) {
    var size = 0
        private set

    val c1 = LongArray(capacity)
    val c2 = GermanStringColumn(capacity)
    val c3 = DoubleArray(capacity)
    val c4 = DoubleArray(capacity)
    val c4Valid = BooleanArray(capacity)
    val c5 = IntArray(capacity)
    val c5Valid = BooleanArray(capacity)
    val c6 = IntArray(capacity)
    val c6Valid = BooleanArray(capacity)
    val c7 = GermanStringColumn(capacity)

    /** Replaces the rows with `source`'s rows, like GD's `row_clear()` + `append(source)`. */
    fun appendFrom(source: Table) {
        val count = source.size
        System.arraycopy(source.c1, 0, c1, 0, count)
        c2.copyFrom(source.c2, count)
        System.arraycopy(source.c3, 0, c3, 0, count)
        System.arraycopy(source.c4, 0, c4, 0, count)
        System.arraycopy(source.c4Valid, 0, c4Valid, 0, count)
        System.arraycopy(source.c5, 0, c5, 0, count)
        System.arraycopy(source.c5Valid, 0, c5Valid, 0, count)
        System.arraycopy(source.c6, 0, c6, 0, count)
        System.arraycopy(source.c6Valid, 0, c6Valid, 0, count)
        c7.copyFrom(source.c7, count)
        size = count
    }

    fun swapRows(a: Int, b: Int) {
        val v1 = c1[a]; c1[a] = c1[b]; c1[b] = v1
        c2.swap(a, b)
        val v3 = c3[a]; c3[a] = c3[b]; c3[b] = v3
        val v4 = c4[a]; c4[a] = c4[b]; c4[b] = v4
        val n4 = c4Valid[a]; c4Valid[a] = c4Valid[b]; c4Valid[b] = n4
        val v5 = c5[a]; c5[a] = c5[b]; c5[b] = v5
        val n5 = c5Valid[a]; c5Valid[a] = c5Valid[b]; c5Valid[b] = n5
        val v6 = c6[a]; c6[a] = c6[b]; c6[b] = v6
        val n6 = c6Valid[a]; c6Valid[a] = c6Valid[b]; c6Valid[b] = n6
        c7.swap(a, b)
    }

    companion object {
        fun fromSource(rows: List<SourceRow>): Table {
            val table = Table(rows.size)
            rows.forEachIndexed { row, source ->
                table.c1[row] = source.c1
                table.c2.set(row, source.c2)
                table.c3[row] = source.c3
                table.c7.set(row, source.c7)
            }
            table.size = rows.size
            return table
        }
    }
}

/** Compares two rows of a table on one key column. */
fun interface RowComparator {
    fun compare(table: Table, a: Int, b: Int): Int
}

/** The sort keys in the C++ benchmark: C1 `uint64`, C2 `string(8)`, C7 `string(50)`. */
enum class Key(val comparator: RowComparator) {
    C1(RowComparator { table, a, b -> table.c1[a].compareTo(table.c1[b]) }),
    C2(RowComparator { table, a, b -> table.c2.compare(a, b) }),
    C7(RowComparator { table, a, b -> table.c7.compare(a, b) }),
}
