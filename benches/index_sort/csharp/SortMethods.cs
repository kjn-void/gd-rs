// C# counterpart of ../cpp/sort_methods_benchmark.cpp.
//
// A 10,000-row table with every column set is sorted on each of its seven columns
// three ways: physical, index, and key. Each operation clears the target, appends
// the source rows, and sorts, like the C++ cases.

using BenchmarkDotNet.Attributes;

namespace IndexSort;

public enum Method
{
    Physical,
    Index,
    Key,
}

/// <summary>Runs one sort method on one column, by column number as in the C++ table.</summary>
public static class ColumnSorts
{
    /// <summary>Sorts <paramref name="table"/> and returns the sorted table, which is a new one for the index and key sorts.</summary>
    public static Table Sort(Table table, Method method, int column) => column switch
    {
        0 => Sort<KeyC1, ulong>(table, method),
        1 => Sort<KeyC2, GermanString>(table, method),
        2 => Sort<KeyC3, double>(table, method),
        3 => Sort<KeyC4, double>(table, method),
        4 => Sort<KeyC5, int>(table, method),
        5 => Sort<KeyC6, uint>(table, method),
        6 => Sort<KeyC7, GermanString>(table, method),
        _ => throw new ArgumentOutOfRangeException(nameof(column)),
    };

    /// <summary>Throws if <paramref name="table"/> is not ordered on <paramref name="column"/>.</summary>
    public static void RequireSorted(Table table, int column)
    {
        switch (column)
        {
            case 0: Sorts.RequireSorted<KeyC1>(table); break;
            case 1: Sorts.RequireSorted<KeyC2>(table); break;
            case 2: Sorts.RequireSorted<KeyC3>(table); break;
            case 3: Sorts.RequireSorted<KeyC4>(table); break;
            case 4: Sorts.RequireSorted<KeyC5>(table); break;
            case 5: Sorts.RequireSorted<KeyC6>(table); break;
            case 6: Sorts.RequireSorted<KeyC7>(table); break;
            default: throw new ArgumentOutOfRangeException(nameof(column));
        }
    }

    private static Table Sort<TKey, TValue>(Table table, Method method)
        where TKey : struct, IKey<TValue>
    {
        switch (method)
        {
            case Method.Physical:
                Sorts.PhysicalSort<TKey>(table);
                return table;
            case Method.Index:
                return Sorts.IndexSort<TKey>(table);
            default:
                return Sorts.KeySort<TKey, TValue>(table);
        }
    }
}

public class SortMethods
{
    private Table _source = null!;
    private Table _target = null!;
    private Method _last;

    [Params(0, 1, 2, 3, 4, 5, 6)]
    public int Column { get; set; }

    [GlobalSetup]
    public void Setup()
    {
        _source = Table.FromSource(Data.GenerateAllColumns(Data.SortMethodsRowCount));
        _target = new Table(Data.SortMethodsRowCount);
    }

    [GlobalCleanup]
    public void Cleanup() => ColumnSorts.RequireSorted(_target, Column);

    [Benchmark] public int Physical() => Run(Method.Physical);
    [Benchmark] public int Index() => Run(Method.Index);
    [Benchmark] public int Key() => Run(Method.Key);

    private int Run(Method method)
    {
        _target.AppendFrom(_source);
        _target = ColumnSorts.Sort(_target, _last = method, Column);
        return _target.Size;
    }
}
