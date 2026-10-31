package indexsort

/**
 * Struct of arrays: one array per column, preallocated to `capacity` rows. C2 and C7
 * hold German strings; C4 to C6 are nullable, like GD's, and keep a validity array
 * next to their values.
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

    /**
     * Builds a new table whose row `k` is this table's row `index[k]`, one column at
     * a time. `index` must be a permutation of the rows.
     */
    fun gather(index: IntArray): Table {
        val sorted = Table(capacity)
        gatherInto(index, sorted)
        return sorted
    }

    fun gatherInto(index: IntArray, sorted: Table) {
        require(this !== sorted) { "gather buffers must differ" }
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
    }

    companion object {
        fun fromSource(rows: List<SourceRow>): Table {
            val table = Table(rows.size)
            rows.forEachIndexed { row, source ->
                table.c1[row] = source.c1
                table.c2.set(row, source.c2)
                table.c3[row] = source.c3
                table.c4[row] = source.c4
                table.c4Valid[row] = true
                table.c5[row] = source.c5
                table.c5Valid[row] = true
                table.c6[row] = source.c6
                table.c6Valid[row] = true
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
 * The sort keys, by column number as in the C++ table. Like GD's sorts, nullable key
 * values must be set.
 */
enum class Key(val column: Int, val comparator: RowComparator) {
    C1(0, RowComparator { table, a, b -> java.lang.Long.compareUnsigned(table.c1[a], table.c1[b]) }),
    C2(1, RowComparator { table, a, b -> table.c2.compare(a, b) }),
    C3(2, RowComparator { table, a, b -> table.c3[a].compareTo(table.c3[b]) }),
    C4(3, RowComparator { table, a, b -> requireSet(table.c4Valid, a, b); table.c4[a].compareTo(table.c4[b]) }),
    C5(4, RowComparator { table, a, b -> requireSet(table.c5Valid, a, b); table.c5[a].compareTo(table.c5[b]) }),
    C6(5, RowComparator { table, a, b -> requireSet(table.c6Valid, a, b); Integer.compareUnsigned(table.c6[a], table.c6[b]) }),
    C7(6, RowComparator { table, a, b -> table.c7.compare(a, b) }),
}

private fun requireSet(valid: BooleanArray, a: Int, b: Int) {
    check(valid[a] && valid[b]) { "sort key is null" }
}
