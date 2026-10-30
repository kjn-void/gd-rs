namespace IndexSort;

/// <summary>Generated values for one row. The six-case benchmark never sets C4 to C6.</summary>
public sealed record SourceRow(ulong C1, string C2, double C3, string C7, double? C4 = null, int? C5 = null, uint? C6 = null);

public static class Data
{
    /// <summary>Row count in the C++ benchmark.</summary>
    public const int RowCount = 1000;

    public const int MillionRows = 1_000_000;

    /// <summary>Row count in the C++ three-sort-method benchmark.</summary>
    public const int SortMethodsRowCount = 10_000;

    /// <summary>
    /// Rows for the six-case benchmark, with C4 to C6 null. Same distributions and
    /// draw order as the C++ test. The engine differs, so the values do too.
    /// </summary>
    public static SourceRow[] GenerateRows(int count) => Generate(count, allColumns: false);

    /// <summary>
    /// Rows for the three-sort-method benchmark, with every column set: C4 a double
    /// like C3, and C5 and C6 a draw like C1 modulo 1000.
    /// </summary>
    public static SourceRow[] GenerateAllColumns(int count) => Generate(count, allColumns: true);

    private static SourceRow[] Generate(int count, bool allColumns)
    {
        var random = new Random(42);
        string Letters(int length) =>
            string.Create(length, random, static (span, random) =>
            {
                for (var index = 0; index < span.Length; index++)
                {
                    span[index] = (char)('a' + random.Next(26));
                }
            });

        var rows = new SourceRow[count];
        for (var row = 0; row < count; row++)
        {
            var c2 = Letters(8);
            var c7 = "01234567890123456789" + Letters(25);
            var c1 = (ulong)random.NextInt64(0, 1_000_001);
            var c3 = random.NextDouble() * 1_000.0;
            rows[row] = new SourceRow(c1, c2, c3, c7);
            if (allColumns)
            {
                rows[row] = rows[row] with
                {
                    C4 = random.NextDouble() * 1_000.0,
                    C5 = (int)(random.NextInt64(0, 1_000_001) % 1000),
                    C6 = (uint)(random.NextInt64(0, 1_000_001) % 1000),
                };
            }
        }
        return rows;
    }
}
