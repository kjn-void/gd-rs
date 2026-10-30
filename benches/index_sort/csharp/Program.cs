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
        CheckEveryColumn(Table.FromSource(Data.GenerateRows(Data.RowCount)));
        CheckDuplicatesAndShortTables();
        CheckGermanOrder();
        CheckDistribution();
        Console.WriteLine("verify: ok");
    }

    /// <summary>Sorts on every column and checks the order and that no row was split or lost.</summary>
    private static void CheckEveryColumn(Table source)
    {
        static string[] RowsOf(Table table) =>
            Enumerable.Range(0, table.Size)
                .Select(row => $"{table.C1[row]} {table.C2[row].Value} {table.C3[row]} {table.C4[row]} {table.C5[row]} {table.C6[row]} {table.C7[row].Value}")
                .ToArray();

        for (var column = 0; column < 7; column++)
        {
            var sorted = ColumnSorts.Sort(source, column);
            ColumnSorts.RequireSorted(sorted, column);
            Require(RowsOf(sorted).Order().SequenceEqual(RowsOf(source).Order()), $"column {column} split or lost rows");
        }
    }

    private static void CheckDuplicatesAndShortTables()
    {
        // C5 and C6 hold only 1,000 distinct values, and C1 is reduced to three.
        var rows = Data.GenerateRows(1000).Select(row => row with { C1 = row.C1 % 3 }).ToArray();
        foreach (var count in new[] { 0, 1, 2, 3, 16, 17, 100, 1000 })
        {
            CheckEveryColumn(Table.FromSource(rows[..count]));
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
            Require(row.C4 is >= 0 and < 1_000 && row.C5 is >= 0 and < 1000 && row.C6 < 1000, "C4 to C6 ranges");
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
