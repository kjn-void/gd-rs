using BenchmarkDotNet.Running;
using IndexSort;

switch (args)
{
    case ["verify"]:
        Verify.Run();
        break;
    default:
        BenchmarkSwitcher.FromAssembly(typeof(Program).Assembly).Run(args);
        break;
}

/// <summary>Correctness checks; the project has no separate test project.</summary>
internal static class Verify
{
    public static void Run()
    {
        CheckSorts();
        CheckDuplicatesAndShortTables();
        CheckSortMethods();
        CheckGermanOrder();
        CheckDistribution();
        Console.WriteLine("verify: ok");
    }

    private static void CheckSorts()
    {
        var fixture = new Fixture(Data.RowCount);
        foreach (var sortCase in Enum.GetValues<Case>())
        {
            fixture.Run(sortCase);
            fixture.RequireSorted(sortCase);
        }

        var source = Table.FromSource(Data.GenerateRows(Data.RowCount));
        static (ulong, string, string)[] RowsOf(Table table) =>
            Enumerable.Range(0, table.Size).Select(row => (table.C1[row], table.C2[row].Value, table.C7[row].Value)).ToArray();

        var physical = new Table(Data.RowCount);
        physical.AppendFrom(source);
        Sorts.PhysicalSort<KeyC7>(physical);
        var index = Sorts.IndexSort<KeyC7>(source);
        Require(RowsOf(physical).SequenceEqual(RowsOf(index)), "physical and index sort ordered rows differently");
        Require(RowsOf(source).Order().SequenceEqual(RowsOf(index).Order()), "index sort split or lost rows");
    }

    private static void CheckDuplicatesAndShortTables()
    {
        var rows = Data.GenerateRows(Data.RowCount).Select(row => row with { C1 = row.C1 % 3 }).ToArray();
        foreach (var count in new[] { 0, 1, 2, 3, 16, 17, 100, Data.RowCount })
        {
            var physical = Table.FromSource(rows[..count]);
            Sorts.PhysicalSort<KeyC1>(physical);
            Sorts.RequireSorted<KeyC1>(physical);
            var index = Sorts.IndexSort<KeyC1>(Table.FromSource(rows[..count]));
            Sorts.RequireSorted<KeyC1>(index);
            Require(index.Size == count, "index sort lost rows");
        }
    }

    private static void CheckSortMethods()
    {
        static string[] RowsOf(Table table) =>
            Enumerable.Range(0, table.Size)
                .Select(row => $"{table.C1[row]} {table.C2[row].Value} {table.C3[row]} {table.C4[row]} {table.C5[row]} {table.C6[row]} {table.C7[row].Value}")
                .ToArray();

        var rows = Data.GenerateAllColumns(Data.SortMethodsRowCount);
        Require(rows.All(row => row.C4 is >= 0 and < 1_000 && row.C5 is >= 0 and < 1000 && row.C6 is < 1000), "all columns set");
        var source = Table.FromSource(rows);
        for (var column = 0; column < 7; column++)
        {
            string[]? expected = null;
            foreach (var method in Enum.GetValues<Method>())
            {
                var table = new Table(source.Size);
                table.AppendFrom(source);
                table = ColumnSorts.Sort(table, method, column);
                ColumnSorts.RequireSorted(table, column);
                var sorted = RowsOf(table);
                expected ??= sorted;
                Require(sorted.SequenceEqual(expected), $"{method} sort on column {column} ordered rows differently");
            }
            Require(expected!.Order().SequenceEqual(RowsOf(source).Order()), $"column {column} split or lost rows");
        }

        var duplicates = Data.GenerateAllColumns(Data.RowCount).Select(row => row with { C1 = row.C1 % 3 }).ToArray();
        foreach (var count in new[] { 0, 1, 2, 3, 16, 17, 100, Data.RowCount })
        {
            foreach (var column in new[] { 0, 5 })
            {
                var key = ColumnSorts.Sort(Table.FromSource(duplicates[..count]), Method.Key, column);
                ColumnSorts.RequireSorted(key, column);
                Require(key.Size == count, "key sort lost rows");
            }
        }
    }

    private static void CheckGermanOrder()
    {
        string[] values = ["", "a", "ab", "ab\0", "abc", "abcd", "abcdefghijkl", "abcdefghijklm", "b", "zz"];
        var strings = values.Select(GermanString.Create).ToArray();
        for (var a = 0; a < values.Length; a++)
        {
            Require(strings[a].Value == values[a], $"GermanString round trip of {values[a]}");
            for (var b = 0; b < values.Length; b++)
            {
                var expected = Math.Sign(string.CompareOrdinal(values[a], values[b]));
                Require(Math.Sign(GermanString.Compare(strings[a], strings[b])) == expected, $"GermanString order of {values[a]} and {values[b]}");
            }
        }
    }

    private static void CheckDistribution()
    {
        foreach (var row in Data.GenerateRows(Data.RowCount))
        {
            Require(row.C2.Length == 8 && row.C7.Length == 45, "string lengths");
            Require(row.C7.StartsWith("01234567890123456789", StringComparison.Ordinal), "C7 prefix");
            Require((row.C2 + row.C7[20..]).All(character => character is >= 'a' and <= 'z'), "lowercase letters");
        }
    }

    private static void Require(bool condition, string message)
    {
        if (!condition)
        {
            throw new InvalidOperationException(message);
        }
    }
}
