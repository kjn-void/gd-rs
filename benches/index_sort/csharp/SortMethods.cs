// C# counterpart of ../cpp/sort_methods_benchmark.cpp.
//
// A 10,000-row table with every column set is sorted on each of its seven columns
// with the port's index sort. Each operation clears the target, appends the source
// rows, and sorts, like the C++ cases.

using BenchmarkDotNet.Attributes;

namespace IndexSort;

/// <summary>Sorts by column number, as in the C++ table.</summary>
public static class ColumnSorts
{
    /// <summary>Returns <paramref name="table"/> sorted on <paramref name="column"/>, as a new table.</summary>
    public static Table Sort(Table table, int column) => column switch
    {
        0 => Sorts.IndexSort<KeyC1>(table),
        1 => Sorts.IndexSort<KeyC2>(table),
        2 => Sorts.IndexSort<KeyC3>(table),
        3 => Sorts.IndexSort<KeyC4>(table),
        4 => Sorts.IndexSort<KeyC5>(table),
        5 => Sorts.IndexSort<KeyC6>(table),
        6 => Sorts.IndexSort<KeyC7>(table),
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
}

public class SortMethods
{
    private Table _source = null!;
    private Table _target = null!;

    [Params(0, 1, 2, 3, 4, 5, 6)]
    public int Column { get; set; }

    [GlobalSetup]
    public void Setup()
    {
        _source = Table.FromSource(Data.GenerateRows(Data.RowCount));
        _target = new Table(Data.RowCount);
    }

    [GlobalCleanup]
    public void Cleanup() => ColumnSorts.RequireSorted(_target, Column);

    [Benchmark]
    public int Index()
    {
        _target.AppendFrom(_source);
        _target = ColumnSorts.Sort(_target, Column);
        return _target.Size;
    }
}
