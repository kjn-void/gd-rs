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

    /**
     * Builds a new table whose row `k` is this table's row `index[k]`, one column at
     * a time. `index` must be a permutation of the rows.
     */
    fun gather(index: IntArray): Table {
        val sorted = Table(capacity)
        for (position in index.indices) sorted.c1[position] = c1[index[position]]
        sorted.c2.gatherFrom(c2, index)
        for (position in index.indices) sorted.c3[position] = c3[index[position]]
        for (position in index.indices) sorted.c4[position] = c4[index[position]]
        for (position in index.indices) sorted.c4Valid[position] = c4Valid[index[position]]
        for (position in index.indices) sorted.c5[position] = c5[index[position]]
        for (position in index.indices) sorted.c5Valid[position] = c5Valid[index[position]]
        for (position in index.indices) sorted.c6[position] = c6[index[position]]
        for (position in index.indices) sorted.c6Valid[position] = c6Valid[index[position]]
        sorted.c7.gatherFrom(c7, index)
        sorted.size = size
        return sorted
    }

    companion object {
        fun fromSource(rows: List<SourceRow>): Table {
            val table = Table(rows.size)
            rows.forEachIndexed { row, source ->
                table.c1[row] = source.c1
                table.c2.set(row, source.c2)
                table.c3[row] = source.c3
                source.c4?.let { table.c4[row] = it; table.c4Valid[row] = true }
                source.c5?.let { table.c5[row] = it; table.c5Valid[row] = true }
                source.c6?.let { table.c6[row] = it; table.c6Valid[row] = true }
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

/**
 * Encodes a row's key as a `Long` whose signed order is the key's order, for the key
 * sort's cache. String keys have none and are cached as a German string column.
 */
fun interface KeyEncoder {
    fun encode(table: Table, row: Int): Long
}

/**
 * The sort keys, by column number as in the C++ table. The six-case benchmark sorts
 * on C1, C2, and C7. Like GD's sorts, nullable key values must be set.
 */
enum class Key(val column: Int, val comparator: RowComparator, val encoder: KeyEncoder?) {
    C1(0, RowComparator { table, a, b -> java.lang.Long.compareUnsigned(table.c1[a], table.c1[b]) }, KeyEncoder { table, row -> table.c1[row] xor Long.MIN_VALUE }),
    C2(1, RowComparator { table, a, b -> table.c2.compare(a, b) }, null),
    C3(2, RowComparator { table, a, b -> table.c3[a].compareTo(table.c3[b]) }, KeyEncoder { table, row -> orderedBits(table.c3[row]) }),
    C4(3, RowComparator { table, a, b -> requireSet(table.c4Valid, a, b); table.c4[a].compareTo(table.c4[b]) }, KeyEncoder { table, row -> requireSet(table.c4Valid, row, row); orderedBits(table.c4[row]) }),
    C5(4, RowComparator { table, a, b -> requireSet(table.c5Valid, a, b); table.c5[a].compareTo(table.c5[b]) }, KeyEncoder { table, row -> requireSet(table.c5Valid, row, row); table.c5[row].toLong() }),
    C6(5, RowComparator { table, a, b -> requireSet(table.c6Valid, a, b); Integer.compareUnsigned(table.c6[a], table.c6[b]) }, KeyEncoder { table, row -> requireSet(table.c6Valid, row, row); table.c6[row].toLong() and 0xffff_ffffL }),
    C7(6, RowComparator { table, a, b -> table.c7.compare(a, b) }, null),
}

private fun requireSet(valid: BooleanArray, a: Int, b: Int) {
    check(valid[a] && valid[b]) { "sort key is null" }
}

/** The bits of `value`, flipped so that signed `Long` order is `Double.compareTo` order. */
private fun orderedBits(value: Double): Long {
    val bits = value.toRawBits()
    return bits xor ((bits shr 63) and Long.MAX_VALUE)
}
