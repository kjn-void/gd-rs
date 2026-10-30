package indexsort

/**
 * Cases 01 to 03: physical sort. Quicksort that compares and swaps whole rows in
 * place. GD's default `sort(column, true)` is a selection sort instead.
 */
fun physicalSort(table: Table, key: RowComparator) {
    quicksort(Rows(table, key), 0, table.size)
}

/**
 * Cases 04 to 06: index sort, after GD's `sort(column, true, tag_sort_index{})`. The
 * same quicksort as [physicalSort] orders an array of row numbers, and the sorted
 * order is applied by building a new table. Returns the new table; `table` is left
 * unchanged. GD instead swaps rows in place along permutation cycles; at 10,000 rows,
 * building the new table took 30 to 48% less time.
 */
fun indexSort(table: Table, key: RowComparator): Table =
    if (table.size < 2) table else table.gather(sortedIndex(table, key))

/**
 * Key sort, after GD's posted `sort(column, true, tag_sort_key{})`: each row's key is
 * copied once into a key array next to an array of row numbers, the same quicksort
 * orders both together, and the sorted order is applied by building a new table as in
 * [indexSort]. Numbers are cached as order-preserving `Long`s; strings as a copy of
 * the German string column. All three sorts produce the same row order.
 */
fun keySort(table: Table, key: Key): Table {
    val count = table.size
    if (count < 2) return table

    val rows = IntArray(count) { it }
    val encoder = key.encoder
    val keys = if (encoder != null) {
        NumberKeys(LongArray(count) { encoder.encode(table, it) }, rows)
    } else {
        val column = if (key == Key.C2) table.c2 else table.c7
        StringKeys(GermanStringColumn(count).apply { copyFrom(column, count) }, rows)
    }
    quicksort(keys, 0, count)
    return table.gather(rows)
}

/** Row numbers in sorted order: `index[k]` is the row that belongs at position `k`. */
private fun sortedIndex(table: Table, key: RowComparator): IntArray {
    val index = IntArray(table.size) { it }
    quicksort(RowNumbers(table, key, index), 0, table.size)
    return index
}

/** What the quicksort orders: positions it can compare and swap. */
private interface Sequence {
    fun compare(a: Int, b: Int): Int
    fun swap(a: Int, b: Int)
}

/** The table's rows themselves. */
private class Rows(private val table: Table, private val key: RowComparator) : Sequence {
    override fun compare(a: Int, b: Int) = key.compare(table, a, b)
    override fun swap(a: Int, b: Int) = table.swapRows(a, b)
}

/** Number keys next to their row numbers, compared by key. */
private class NumberKeys(private val keys: LongArray, private val rows: IntArray) : Sequence {
    override fun compare(a: Int, b: Int) = keys[a].compareTo(keys[b])

    override fun swap(a: Int, b: Int) {
        val key = keys[a]; keys[a] = keys[b]; keys[b] = key
        val row = rows[a]; rows[a] = rows[b]; rows[b] = row
    }
}

/** String keys next to their row numbers, compared by key. */
private class StringKeys(private val keys: GermanStringColumn, private val rows: IntArray) : Sequence {
    override fun compare(a: Int, b: Int) = keys.compare(a, b)

    override fun swap(a: Int, b: Int) {
        keys.swap(a, b)
        val row = rows[a]; rows[a] = rows[b]; rows[b] = row
    }
}

/** Row numbers, compared through the rows they name. */
private class RowNumbers(
    private val table: Table,
    private val key: RowComparator,
    private val index: IntArray,
) : Sequence {
    override fun compare(a: Int, b: Int) = key.compare(table, index[a], index[b])

    override fun swap(a: Int, b: Int) {
        val row = index[a]
        index[a] = index[b]
        index[b] = row
    }
}

/** Ranges this short are finished with insertion sort. */
private const val INSERTION_SORT_LENGTH = 16

/**
 * Sorts positions `[from, to)`. The pivot is the median of the first, middle, and
 * last positions. Partitioning stops on keys equal to the pivot, so duplicate keys
 * still split evenly. The sort recurses into the smaller part only and uses
 * insertion sort for short ranges. It is not stable.
 */
private fun quicksort(sequence: Sequence, from: Int, to: Int) {
    var low = from
    var high = to
    while (high - low > INSERTION_SORT_LENGTH) {
        val pivot = partition(sequence, low, high)
        if (pivot - low < high - pivot) {
            quicksort(sequence, low, pivot)
            low = pivot + 1
        } else {
            quicksort(sequence, pivot + 1, high)
            high = pivot
        }
    }
    insertionSort(sequence, low, high)
}

/**
 * Partitions `[from, to)` around a median-of-three pivot and returns the pivot's
 * final position. Needs at least three positions.
 */
private fun partition(sequence: Sequence, from: Int, to: Int): Int {
    val middle = from + (to - from) / 2
    val last = to - 1
    if (sequence.compare(middle, from) < 0) sequence.swap(middle, from)
    if (sequence.compare(last, from) < 0) sequence.swap(last, from)
    if (sequence.compare(last, middle) < 0) sequence.swap(last, middle)
    // The pivot moves to `from`; the smaller and larger samples bound both scans.
    sequence.swap(from, middle)

    var left = from
    var right = to
    while (true) {
        left++
        while (sequence.compare(left, from) < 0) left++
        right--
        while (sequence.compare(from, right) < 0) right--
        if (left >= right) break
        sequence.swap(left, right)
    }
    sequence.swap(from, right)
    return right
}

/** Sorts `[from, to)` by swapping adjacent positions. */
private fun insertionSort(sequence: Sequence, from: Int, to: Int) {
    for (next in from + 1 until to) {
        var position = next
        while (position > from && sequence.compare(position, position - 1) < 0) {
            sequence.swap(position, position - 1)
            position--
        }
    }
}

/** Fails if `table` is not ordered on `key`; benchmarks call it after measuring. */
fun requireSorted(table: Table, key: RowComparator) {
    for (row in 1 until table.size) {
        check(key.compare(table, row - 1, row) <= 0) { "not sorted at row $row" }
    }
}
