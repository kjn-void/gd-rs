package indexsort

/**
 * Sorts `table` on `key`, after GD's `sort(column, true, tag_sort_index{})`. A
 * quicksort orders an array of row numbers, comparing through the rows they name, and
 * the sorted order is applied by building a new table. Returns the new table; `table`
 * is left unchanged. The sort is not stable.
 *
 * The port has this one sort, the fastest of the physical, index, and key sorts
 * measured at commit 906a410.
 */
fun indexSort(table: Table, key: RowComparator): Table {
    if (table.size < 2) return table
    val index = IntArray(table.size) { it }
    RowNumbers(table, key, index).quicksort(0, table.size)
    return table.gather(index)
}

internal fun indexInto(table: Table, key: RowComparator, index: IntArray, destination: Table) {
    for (row in index.indices) index[row] = row
    if (table.size > 1) RowNumbers(table, key, index).quicksort(0, table.size)
    table.gatherInto(index, destination)
}

/** Fails if `table` is not ordered on `key`; benchmarks call it after measuring. */
fun requireSorted(table: Table, key: RowComparator) {
    for (row in 1 until table.size) {
        check(key.compare(table, row - 1, row) <= 0) { "not sorted at row $row" }
    }
}

/** Ranges this short are finished with insertion sort. */
internal const val INSERTION_SORT_LENGTH = 16

/** Row numbers, compared through the rows they name. */
private class RowNumbers(
    private val table: Table,
    private val key: RowComparator,
    private val index: IntArray,
) {
    private fun less(a: Int, b: Int) = key.compare(table, index[a], index[b]) < 0

    private fun swap(a: Int, b: Int) {
        val row = index[a]
        index[a] = index[b]
        index[b] = row
    }

    /**
     * Sorts positions `[from, to)`. The pivot is the median of the first, middle, and
     * last positions. Partitioning stops on keys equal to the pivot, so duplicate keys
     * still split evenly. The sort recurses into the smaller part only and uses
     * insertion sort for short ranges.
     */
    fun quicksort(from: Int, to: Int) {
        var low = from
        var high = to
        while (high - low > INSERTION_SORT_LENGTH) {
            val pivot = partition(low, high)
            if (pivot - low < high - pivot) {
                quicksort(low, pivot)
                low = pivot + 1
            } else {
                quicksort(pivot + 1, high)
                high = pivot
            }
        }
        insertionSort(low, high)
    }

    /**
     * Partitions `[from, to)` around a median-of-three pivot and returns the pivot's
     * final position. Needs at least three positions.
     */
    private fun partition(from: Int, to: Int): Int {
        val middle = from + (to - from) / 2
        val last = to - 1
        if (less(middle, from)) swap(middle, from)
        if (less(last, from)) swap(last, from)
        if (less(last, middle)) swap(last, middle)
        // The pivot moves to `from`; the smaller and larger samples bound both scans.
        swap(from, middle)

        var left = from
        var right = to
        while (true) {
            left++
            while (less(left, from)) left++
            right--
            while (less(from, right)) right--
            if (left >= right) break
            swap(left, right)
        }
        swap(from, right)
        return right
    }

    /** Sorts `[from, to)` by swapping adjacent positions. */
    private fun insertionSort(from: Int, to: Int) {
        for (next in from + 1 until to) {
            var position = next
            while (position > from && less(position, position - 1)) {
                swap(position, position - 1)
                position--
            }
        }
    }
}
