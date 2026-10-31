package indexsort

/** Measured per-column policy; C7 keeps its allocating index sort. */
fun optimizedSort(table: Table, key: Key, workspace: SortWorkspace): Table = when (key) {
    Key.C1 -> workspace.reuse(table, key)
    Key.C2, Key.C7 -> indexSort(table, key.comparator)
    else -> workspace.sort(table, key, "cached")
}

/** Reuses an index, primitive numeric key cache, and destination table. */
class SortWorkspace(rows: Int) {
    private val index = IntArray(rows)
    private val numbers = LongArray(rows)
    private var spare = Table(rows)

    /** Direct index-reuse entry avoids the generic strategy dispatcher. */
    fun reuse(table: Table, key: Key): Table {
        require(table.size == index.size) { "workspace row count differs" }
        indexInto(table, key.comparator, index, spare)
        val result = spare
        spare = table
        return result
    }

    fun sort(table: Table, key: Key, variant: String): Table {
        require(table.size == index.size) { "workspace row count differs" }
        require(variant == "reuse" || variant == "validated" || variant == "cached") { "unknown variant" }
        if (variant == "validated") validate(table, key)
        if (variant == "cached" && key != Key.C2 && key != Key.C7) {
            for (row in index.indices) {
                index[row] = row
                numbers[row] = when (key) {
                    Key.C1 -> table.c1[row]
                    Key.C3 -> doubleKey(table.c3[row])
                    Key.C4 -> { check(table.c4Valid[row]) { "sort key is null" }; doubleKey(table.c4[row]) }
                    Key.C5 -> { check(table.c5Valid[row]) { "sort key is null" }; table.c5[row].toLong() xor Long.MIN_VALUE }
                    Key.C6 -> { check(table.c6Valid[row]) { "sort key is null" }; Integer.toUnsignedLong(table.c6[row]) }
                    else -> error("not numeric")
                }
            }
            CachedNumbers(numbers, index).quicksort(0, table.size)
            table.gatherInto(index, spare)
        } else {
            val comparator = if (variant == "reuse") key.comparator else when (key) {
                Key.C4 -> VALID_C4
                Key.C5 -> VALID_C5
                Key.C6 -> VALID_C6
                else -> key.comparator
            }
            indexInto(table, comparator, index, spare)
        }
        val result = spare
        spare = table
        return result
    }

    private fun validate(table: Table, key: Key) {
        val valid = when (key) {
            Key.C4 -> table.c4Valid
            Key.C5 -> table.c5Valid
            Key.C6 -> table.c6Valid
            else -> return
        }
        for (row in index.indices) check(valid[row]) { "sort key is null" }
    }

    private companion object {
        val VALID_C4 = RowComparator { t, a, b -> t.c4[a].compareTo(t.c4[b]) }
        val VALID_C5 = RowComparator { t, a, b -> t.c5[a].compareTo(t.c5[b]) }
        val VALID_C6 = RowComparator { t, a, b -> Integer.compareUnsigned(t.c6[a], t.c6[b]) }
        fun doubleKey(value: Double): Long {
            val bits = java.lang.Double.doubleToLongBits(value)
            return bits xor (if (bits < 0) -1L else Long.MIN_VALUE)
        }
    }
}

private class CachedNumbers(private val keys: LongArray, private val index: IntArray) {
    private fun less(a: Int, b: Int) = java.lang.Long.compareUnsigned(keys[a], keys[b]) < 0
    private fun swap(a: Int, b: Int) {
        val key = keys[a]; keys[a] = keys[b]; keys[b] = key
        val row = index[a]; index[a] = index[b]; index[b] = row
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
