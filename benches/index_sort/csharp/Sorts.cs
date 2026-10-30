namespace IndexSort;

public static class Sorts
{
    /// <summary>
    /// Cases 01 to 03: physical sort. Quicksort that compares and swaps whole rows in
    /// place. GD's default <c>sort(column, true)</c> is a selection sort instead.
    /// </summary>
    public static void PhysicalSort<TKey>(Table table)
        where TKey : struct, IKey
    {
        var rows = new Rows<TKey>(table);
        Quicksort(ref rows, 0, table.Size);
    }

    /// <summary>
    /// Cases 04 to 06: index sort, after GD's <c>sort(column, true, tag_sort_index{})</c>.
    /// The same quicksort as <see cref="PhysicalSort{TKey}"/> orders an array of row
    /// numbers, and the sorted order is applied by building a new table. Returns the new
    /// table; <paramref name="table"/> is left unchanged. GD instead swaps rows in place
    /// along permutation cycles; at 10,000 rows, building the new table took 9 to 18%
    /// less time.
    /// </summary>
    public static Table IndexSort<TKey>(Table table)
        where TKey : struct, IKey
    {
        return table.Size < 2 ? table : table.Gather(SortedIndex<TKey>(table));
    }

    /// <summary>Row numbers in sorted order: <c>index[k]</c> is the row that belongs at position <c>k</c>.</summary>
    private static int[] SortedIndex<TKey>(Table table)
        where TKey : struct, IKey
    {
        var count = table.Size;
        var index = new int[count];
        for (var row = 0; row < count; row++)
        {
            index[row] = row;
        }
        var rowNumbers = new RowNumbers<TKey>(table, index);
        Quicksort(ref rowNumbers, 0, count);
        return index;
    }

    /// <summary>Throws if <paramref name="table"/> is not ordered on <typeparamref name="TKey"/>.</summary>
    public static void RequireSorted<TKey>(Table table)
        where TKey : struct, IKey
    {
        for (var row = 1; row < table.Size; row++)
        {
            if (TKey.Compare(table, row - 1, row) > 0)
            {
                throw new InvalidOperationException($"{typeof(TKey).Name} is not sorted at row {row}");
            }
        }
    }

    /// <summary>What the quicksort orders: positions it can compare and swap.</summary>
    private interface ISequence
    {
        int Compare(int a, int b);
        void Swap(int a, int b);
    }

    /// <summary>The table's rows themselves.</summary>
    private readonly struct Rows<TKey>(Table table) : ISequence
        where TKey : struct, IKey
    {
        public int Compare(int a, int b) => TKey.Compare(table, a, b);
        public void Swap(int a, int b) => table.SwapRows(a, b);
    }

    /// <summary>Row numbers, compared through the rows they name.</summary>
    private readonly struct RowNumbers<TKey>(Table table, int[] index) : ISequence
        where TKey : struct, IKey
    {
        public int Compare(int a, int b) => TKey.Compare(table, index[a], index[b]);
        public void Swap(int a, int b) => (index[a], index[b]) = (index[b], index[a]);
    }

    /// <summary>Ranges this short are finished with insertion sort.</summary>
    private const int InsertionSortLength = 16;

    /// <summary>
    /// Sorts positions <c>[from, to)</c>. The pivot is the median of the first, middle,
    /// and last positions. Partitioning stops on keys equal to the pivot, so duplicate
    /// keys still split evenly. The sort recurses into the smaller part only and uses
    /// insertion sort for short ranges. It is not stable.
    /// </summary>
    private static void Quicksort<TSequence>(ref TSequence sequence, int from, int to)
        where TSequence : struct, ISequence
    {
        while (to - from > InsertionSortLength)
        {
            var pivot = Partition(ref sequence, from, to);
            if (pivot - from < to - pivot)
            {
                Quicksort(ref sequence, from, pivot);
                from = pivot + 1;
            }
            else
            {
                Quicksort(ref sequence, pivot + 1, to);
                to = pivot;
            }
        }
        InsertionSort(ref sequence, from, to);
    }

    /// <summary>
    /// Partitions <c>[from, to)</c> around a median-of-three pivot and returns the
    /// pivot's final position. Needs at least three positions.
    /// </summary>
    private static int Partition<TSequence>(ref TSequence sequence, int from, int to)
        where TSequence : struct, ISequence
    {
        var middle = from + (to - from) / 2;
        var last = to - 1;
        if (sequence.Compare(middle, from) < 0) sequence.Swap(middle, from);
        if (sequence.Compare(last, from) < 0) sequence.Swap(last, from);
        if (sequence.Compare(last, middle) < 0) sequence.Swap(last, middle);
        // The pivot moves to `from`; the smaller and larger samples bound both scans.
        sequence.Swap(from, middle);

        var left = from;
        var right = to;
        while (true)
        {
            left++;
            while (sequence.Compare(left, from) < 0) left++;
            right--;
            while (sequence.Compare(from, right) < 0) right--;
            if (left >= right) break;
            sequence.Swap(left, right);
        }
        sequence.Swap(from, right);
        return right;
    }

    /// <summary>Sorts <c>[from, to)</c> by swapping adjacent positions.</summary>
    private static void InsertionSort<TSequence>(ref TSequence sequence, int from, int to)
        where TSequence : struct, ISequence
    {
        for (var next = from + 1; next < to; next++)
        {
            for (var position = next; position > from && sequence.Compare(position, position - 1) < 0; position--)
            {
                sequence.Swap(position, position - 1);
            }
        }
    }
}
