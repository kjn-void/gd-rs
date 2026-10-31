namespace IndexSort;

/// <summary>Reusable destination/index buffers and typed numeric key caches.</summary>
public sealed class SortWorkspace(int rows)
{
    private Table _spare = new(rows);
    private readonly int[] _index = new int[rows];
    private readonly Pair<ulong>[] _u64 = new Pair<ulong>[rows];
    private readonly Pair<double>[] _f64 = new Pair<double>[rows];
    private readonly Pair<int>[] _i32 = new Pair<int>[rows];
    private readonly Pair<uint>[] _u32 = new Pair<uint>[rows];

    public Table Sort(Table table, int column, string variant)
    {
        if (table.Size != _index.Length) throw new ArgumentException("workspace row count differs");
        if (variant is not ("reuse" or "validated" or "cached")) throw new ArgumentException("unknown variant");
        if (variant == "validated") Validate(table, column);
        if (variant == "cached" && column is not (1 or 6))
        {
            switch (column)
            {
                case 0: Cache(table.C1, _u64); break;
                case 2: Cache(table.C3, _f64); break;
                case 3: CacheNullable(table.C4, _f64); break;
                case 4: CacheNullable(table.C5, _i32); break;
                case 5: CacheNullable(table.C6, _u32); break;
                default: throw new ArgumentOutOfRangeException(nameof(column));
            }
            table.GatherInto(_index, _spare);
        }
        else
        {
            switch (column)
            {
                case 0: Sorts.IndexInto<KeyC1>(table, _index, _spare); break;
                case 1: Sorts.IndexInto<KeyC2>(table, _index, _spare); break;
                case 2: Sorts.IndexInto<KeyC3>(table, _index, _spare); break;
                case 3 when variant == "reuse": Sorts.IndexInto<KeyC4>(table, _index, _spare); break;
                case 4 when variant == "reuse": Sorts.IndexInto<KeyC5>(table, _index, _spare); break;
                case 5 when variant == "reuse": Sorts.IndexInto<KeyC6>(table, _index, _spare); break;
                case 3: Sorts.IndexInto<ValidC4>(table, _index, _spare); break;
                case 4: Sorts.IndexInto<ValidC5>(table, _index, _spare); break;
                case 5: Sorts.IndexInto<ValidC6>(table, _index, _spare); break;
                case 6: Sorts.IndexInto<KeyC7>(table, _index, _spare); break;
                default: throw new ArgumentOutOfRangeException(nameof(column));
            }
        }
        var result = _spare;
        _spare = table;
        return result;
    }

    private static void Validate(Table table, int column)
    {
        if (column is not (3 or 4 or 5)) return;
        for (var row = 0; row < table.Size; row++)
        {
            var valid = column switch
            {
                3 => table.C4[row].HasValue,
                4 => table.C5[row].HasValue,
                5 => table.C6[row].HasValue,
                _ => true,
            };
            if (!valid) throw new InvalidOperationException("sort key is null");
        }
    }

    private void Cache<T>(T[] values, Pair<T>[] pairs) where T : struct, IComparable<T>
    {
        for (var row = 0; row < _index.Length; row++) pairs[row] = new(values[row], row);
        Finish(pairs);
    }

    private void CacheNullable<T>(T?[] values, Pair<T>[] pairs) where T : struct, IComparable<T>
    {
        for (var row = 0; row < _index.Length; row++) pairs[row] = new(Key.Set(values[row]), row);
        Finish(pairs);
    }

    private void Finish<T>(Pair<T>[] pairs) where T : struct, IComparable<T>
    {
        Quicksort(pairs, 0, pairs.Length);
        for (var row = 0; row < _index.Length; row++) _index[row] = pairs[row].Row;
    }

    private readonly record struct Pair<T>(T Key, int Row);
    private static void Quicksort<T>(Pair<T>[] pairs, int from, int to) where T : struct, IComparable<T>
    {
        static bool Less(Pair<T>[] values, int a, int b) => values[a].Key.CompareTo(values[b].Key) < 0;
        static void Swap(Pair<T>[] values, int a, int b) => (values[a], values[b]) = (values[b], values[a]);
        while (to - from > 16)
        {
            var middle = from + (to - from) / 2;
            var last = to - 1;
            if (Less(pairs, middle, from)) Swap(pairs, middle, from);
            if (Less(pairs, last, from)) Swap(pairs, last, from);
            if (Less(pairs, last, middle)) Swap(pairs, last, middle);
            Swap(pairs, from, middle);
            var left = from;
            var right = to;
            while (true)
            {
                do { left++; } while (Less(pairs, left, from));
                do { right--; } while (Less(pairs, from, right));
                if (left >= right) break;
                Swap(pairs, left, right);
            }
            Swap(pairs, from, right);
            if (right - from < to - right)
            {
                Quicksort(pairs, from, right);
                from = right + 1;
            }
            else
            {
                Quicksort(pairs, right + 1, to);
                to = right;
            }
        }
        for (var next = from + 1; next < to; next++)
        {
            for (var position = next; position > from && Less(pairs, position, position - 1); position--)
                Swap(pairs, position, position - 1);
        }
    }
    private readonly struct ValidC4 : IKey
    {
        public static int Compare(Table t, int a, int b) => t.C4[a].GetValueOrDefault().CompareTo(t.C4[b].GetValueOrDefault());
    }
    private readonly struct ValidC5 : IKey
    {
        public static int Compare(Table t, int a, int b) => t.C5[a].GetValueOrDefault().CompareTo(t.C5[b].GetValueOrDefault());
    }
    private readonly struct ValidC6 : IKey
    {
        public static int Compare(Table t, int a, int b) => t.C6[a].GetValueOrDefault().CompareTo(t.C6[b].GetValueOrDefault());
    }
}
