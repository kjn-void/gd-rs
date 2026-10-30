namespace IndexSort;

/// <summary>Generated values for one row.</summary>
public sealed record SourceRow(ulong C1, string C2, double C3, double C4, int C5, uint C6, string C7);

public static class Data
{
    /// <summary>Row count in the C++ benchmark.</summary>
    public const int RowCount = 10_000;

    /// <summary>
    /// Same distributions and draw order as the C++ test, with every column set: C4 a
    /// double like C3, and C5 and C6 a draw like C1 modulo 1000. The engine differs, so
    /// the values do too.
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
            var c4 = random.NextDouble() * 1_000.0;
            var c5 = (int)(random.NextInt64(0, 1_000_001) % 1000);
            var c6 = (uint)(random.NextInt64(0, 1_000_001) % 1000);
            rows[row] = new SourceRow(c1, c2, c3, c4, c5, c6, c7);
        }
        return rows;
    }
}
