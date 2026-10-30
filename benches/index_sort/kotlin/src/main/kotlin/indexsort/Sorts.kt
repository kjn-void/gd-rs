package indexsort

/**
 * Cases 01 to 03: physical sort. Quicksort that compares and swaps whole rows in
 * place. GD's default `sort(column, true)` is a selection sort instead.
 */
fun physicalSort(table: Table, key: RowComparator) {
    quicksort(Rows(table, key), 0, table.size)
}

/**
 * Cases 04 to 06: index sort, like GD's `sort(column, true, tag_sort_index{})`. The
 * same quicksort as [physicalSort] orders an array of row numbers, then the
 * permutation is applied in place by following cycles. Both sorts make the same
 * comparisons and produce the same row order; only the data they move differs.
 */
fun indexSort(table: Table, key: RowComparator) {
    val count = table.size
    if (count < 2) return

    val index = IntArray(count) { it }
    quicksort(RowNumbers(table, key, index), 0, count)

    // index[k] is the original row that should end up at position k.
    for (position in 0 until count) {
        var source = index[position]
        while (source < position) source = index[source] // follow rows that were already moved
        if (source != position) table.swapRows(position, source)
    }
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
