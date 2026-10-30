// C# counterpart of ../cpp/index_sort_benchmark.cpp.
//
// Each operation clears the target, appends the source rows, and sorts on one
// column, like the C++ cases. Cases 07 to 09 build a new table instead of swapping
// rows, and run only at 10,000 rows.

using BenchmarkDotNet.Attributes;

namespace IndexSort;

public enum Case
{
    AppendOnly,
    PhysicalUint64,
    PhysicalString8,
    PhysicalString50,
    IndexUint64,
    IndexString8,
    IndexString50,
    IndexGatherUint64,
    IndexGatherString8,
    IndexGatherString50,
}

/// <summary>Source and target tables for one row count.</summary>
public sealed class Fixture(int rows)
{
    private readonly Table _source = Table.FromSource(Data.GenerateRows(rows));
    private Table _target = new(rows);

    public int Run(Case sortCase)
    {
        _target.AppendFrom(_source);
        switch (sortCase)
        {
            case Case.AppendOnly: break;
            case Case.PhysicalUint64: Sorts.PhysicalSort<KeyC1>(_target); break;
            case Case.PhysicalString8: Sorts.PhysicalSort<KeyC2>(_target); break;
            case Case.PhysicalString50: Sorts.PhysicalSort<KeyC7>(_target); break;
            case Case.IndexUint64: Sorts.IndexSort<KeyC1>(_target); break;
            case Case.IndexString8: Sorts.IndexSort<KeyC2>(_target); break;
            case Case.IndexString50: Sorts.IndexSort<KeyC7>(_target); break;
            case Case.IndexGatherUint64: _target = Sorts.IndexGatherSort<KeyC1>(_target); break;
            case Case.IndexGatherString8: _target = Sorts.IndexGatherSort<KeyC2>(_target); break;
            case Case.IndexGatherString50: _target = Sorts.IndexGatherSort<KeyC7>(_target); break;
        }
        return _target.Size;
    }

    /// <summary>Throws if the target is not ordered for <paramref name="sortCase"/>.</summary>
    public void RequireSorted(Case sortCase)
    {
        switch (sortCase)
        {
            case Case.PhysicalUint64 or Case.IndexUint64 or Case.IndexGatherUint64: Sorts.RequireSorted<KeyC1>(_target); break;
            case Case.PhysicalString8 or Case.IndexString8 or Case.IndexGatherString8: Sorts.RequireSorted<KeyC2>(_target); break;
            case Case.PhysicalString50 or Case.IndexString50 or Case.IndexGatherString50: Sorts.RequireSorted<KeyC7>(_target); break;
        }
    }
}

public class ThousandRows
{
    private Fixture _fixture = null!;
    private Case _last;

    [GlobalSetup]
    public void Setup() => _fixture = new Fixture(Data.RowCount);

    [GlobalCleanup]
    public void Cleanup() => _fixture.RequireSorted(_last);

    [Benchmark] public int Sort00AppendOnly() => _fixture.Run(_last = Case.AppendOnly);
    [Benchmark] public int Sort01PhysicalUint64() => _fixture.Run(_last = Case.PhysicalUint64);
    [Benchmark] public int Sort02PhysicalString8() => _fixture.Run(_last = Case.PhysicalString8);
    [Benchmark] public int Sort03PhysicalString50() => _fixture.Run(_last = Case.PhysicalString50);
    [Benchmark] public int Sort04IndexUint64() => _fixture.Run(_last = Case.IndexUint64);
    [Benchmark] public int Sort05IndexString8() => _fixture.Run(_last = Case.IndexString8);
    [Benchmark] public int Sort06IndexString50() => _fixture.Run(_last = Case.IndexString50);
}

/// <summary>Swapping rows in place versus building a new table, after the same index sort.</summary>
public class TenThousandRows
{
    private Fixture _fixture = null!;
    private Case _last;

    [GlobalSetup]
    public void Setup() => _fixture = new Fixture(Data.TenThousandRows);

    [GlobalCleanup]
    public void Cleanup() => _fixture.RequireSorted(_last);

    [Benchmark] public int Sort04IndexUint64() => _fixture.Run(_last = Case.IndexUint64);
    [Benchmark] public int Sort05IndexString8() => _fixture.Run(_last = Case.IndexString8);
    [Benchmark] public int Sort06IndexString50() => _fixture.Run(_last = Case.IndexString50);
    [Benchmark] public int Sort07IndexGatherUint64() => _fixture.Run(_last = Case.IndexGatherUint64);
    [Benchmark] public int Sort08IndexGatherString8() => _fixture.Run(_last = Case.IndexGatherString8);
    [Benchmark] public int Sort09IndexGatherString50() => _fixture.Run(_last = Case.IndexGatherString50);
}

[SimpleJob(warmupCount: 3, iterationCount: 10)]
public class MillionRows
{
    private Fixture _fixture = null!;
    private Case _last;

    [GlobalSetup]
    public void Setup() => _fixture = new Fixture(Data.MillionRows);

    [GlobalCleanup]
    public void Cleanup() => _fixture.RequireSorted(_last);

    [Benchmark] public int Sort00AppendOnly() => _fixture.Run(_last = Case.AppendOnly);
    [Benchmark] public int Sort01PhysicalUint64() => _fixture.Run(_last = Case.PhysicalUint64);
    [Benchmark] public int Sort02PhysicalString8() => _fixture.Run(_last = Case.PhysicalString8);
    [Benchmark] public int Sort03PhysicalString50() => _fixture.Run(_last = Case.PhysicalString50);
    [Benchmark] public int Sort04IndexUint64() => _fixture.Run(_last = Case.IndexUint64);
    [Benchmark] public int Sort05IndexString8() => _fixture.Run(_last = Case.IndexString8);
    [Benchmark] public int Sort06IndexString50() => _fixture.Run(_last = Case.IndexString50);
}
