namespace IndexSort;

/// <summary>Generated values for one row; C4 to C6 are never set, like the C++ test.</summary>
public sealed record SourceRow(ulong C1, string C2, double C3, string C7);

public static class Data
{
    /// <summary>Row count in the C++ benchmark.</summary>
    public const int RowCount = 1000;

    /// <summary>Row count in the swap versus gather comparison.</summary>
    public const int TenThousandRows = 10_000;

    public const int MillionRows = 1_000_000;

    /// <summary>
    /// Same distributions and draw order as the C++ test. The engine differs, so the
    /// values do too.
    /// </summary>
    public static SourceRow[] GenerateRows(int count)
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
        }
        return rows;
    }
}
