namespace IndexSort;

public static class Sorts
{
    /// <summary>
    /// Sorts <paramref name="table"/> on <typeparamref name="TKey"/>, after GD's
    /// <c>sort(column, true, tag_sort_index{})</c>. A quicksort orders an array of row
    /// numbers, comparing through the rows they name, and the sorted order is applied
    /// by building a new table. Returns the new table; <paramref name="table"/> is left
    /// unchanged. The sort is not stable.
    /// </summary>
    /// <remarks>
    /// The port has this one sort, the fastest of the physical, index, and key sorts
    /// measured at commit 906a410.
    /// </remarks>
    public static Table IndexSort<TKey>(Table table)
        where TKey : struct, IKey
    {
        var count = table.Size;
        if (count < 2)
        {
            return table;
        }

        var index = new int[count];
        for (var row = 0; row < count; row++)
        {
            index[row] = row;
        }
        Quicksort<TKey>(table, index, 0, count);
        return table.Gather(index);
    }

    public static void IndexInto<TKey>(Table table, int[] index, Table destination)
        where TKey : struct, IKey
    {
        for (var row = 0; row < table.Size; row++) index[row] = row;
        if (table.Size > 1) Quicksort<TKey>(table, index, 0, table.Size);
        table.GatherInto(index, destination);
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

    /// <summary>Ranges this short are finished with insertion sort.</summary>
    private const int InsertionSortLength = 16;

    private static bool Less<TKey>(Table table, int[] index, int a, int b)
        where TKey : struct, IKey
        => TKey.Compare(table, index[a], index[b]) < 0;

    private static void Swap(int[] index, int a, int b) => (index[a], index[b]) = (index[b], index[a]);

    /// <summary>
    /// Sorts positions <c>[from, to)</c> of <paramref name="index"/>. The pivot is the
    /// median of the first, middle, and last positions. Partitioning stops on keys equal
    /// to the pivot, so duplicate keys still split evenly. The sort recurses into the
    /// smaller part only and uses insertion sort for short ranges.
    /// </summary>
    private static void Quicksort<TKey>(Table table, int[] index, int from, int to)
        where TKey : struct, IKey
    {
        while (to - from > InsertionSortLength)
        {
            var pivot = Partition<TKey>(table, index, from, to);
            if (pivot - from < to - pivot)
            {
                Quicksort<TKey>(table, index, from, pivot);
                from = pivot + 1;
            }
            else
            {
                Quicksort<TKey>(table, index, pivot + 1, to);
                to = pivot;
            }
        }
        InsertionSort<TKey>(table, index, from, to);
    }

    /// <summary>
    /// Partitions <c>[from, to)</c> around a median-of-three pivot and returns the
    /// pivot's final position. Needs at least three positions.
    /// </summary>
    private static int Partition<TKey>(Table table, int[] index, int from, int to)
        where TKey : struct, IKey
    {
        var middle = from + (to - from) / 2;
        var last = to - 1;
        if (Less<TKey>(table, index, middle, from)) Swap(index, middle, from);
        if (Less<TKey>(table, index, last, from)) Swap(index, last, from);
        if (Less<TKey>(table, index, last, middle)) Swap(index, last, middle);
        // The pivot moves to `from`; the smaller and larger samples bound both scans.
        Swap(index, from, middle);

        var left = from;
        var right = to;
        while (true)
        {
            left++;
            while (Less<TKey>(table, index, left, from)) left++;
            right--;
            while (Less<TKey>(table, index, from, right)) right--;
            if (left >= right) break;
            Swap(index, left, right);
        }
        Swap(index, from, right);
        return right;
    }

    /// <summary>Sorts <c>[from, to)</c> by swapping adjacent positions.</summary>
    private static void InsertionSort<TKey>(Table table, int[] index, int from, int to)
        where TKey : struct, IKey
    {
        for (var next = from + 1; next < to; next++)
        {
            for (var position = next; position > from && Less<TKey>(table, index, position, position - 1); position--)
            {
                Swap(index, position, position - 1);
            }
        }
    }
}
