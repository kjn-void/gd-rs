namespace IndexSort;

/// <summary>
/// Struct of arrays: one array per column, preallocated to the row capacity. C2 and C7
/// hold German strings.
/// </summary>
public sealed class Table
{
    public Table(int capacity)
    {
        C1 = new ulong[capacity];
        C2 = new GermanString[capacity];
        C3 = new double[capacity];
        C4 = new double?[capacity];
        C5 = new int?[capacity];
        C6 = new uint?[capacity];
        C7 = new GermanString[capacity];
    }

    public int Size { get; private set; }

    public ulong[] C1 { get; }
    public GermanString[] C2 { get; }
    public double[] C3 { get; }
    public double?[] C4 { get; }
    public int?[] C5 { get; }
    public uint?[] C6 { get; }
    public GermanString[] C7 { get; }

    public static Table FromSource(SourceRow[] rows)
    {
        var table = new Table(rows.Length);
        for (var row = 0; row < rows.Length; row++)
        {
            table.C1[row] = rows[row].C1;
            table.C2[row] = GermanString.Create(rows[row].C2);
            table.C3[row] = rows[row].C3;
            table.C7[row] = GermanString.Create(rows[row].C7);
        }
        table.Size = rows.Length;
        return table;
    }

    /// <summary>
    /// Replaces the rows with <paramref name="source"/>'s rows, like GD's
    /// <c>row_clear()</c> followed by <c>append(source)</c>. Text is shared, not copied.
    /// </summary>
    public void AppendFrom(Table source)
    {
        var count = source.Size;
        source.C1.AsSpan(0, count).CopyTo(C1);
        source.C2.AsSpan(0, count).CopyTo(C2);
        source.C3.AsSpan(0, count).CopyTo(C3);
        source.C4.AsSpan(0, count).CopyTo(C4);
        source.C5.AsSpan(0, count).CopyTo(C5);
        source.C6.AsSpan(0, count).CopyTo(C6);
        source.C7.AsSpan(0, count).CopyTo(C7);
        Size = count;
    }

    public void SwapRows(int a, int b)
    {
        Swap(C1, a, b);
        Swap(C2, a, b);
        Swap(C3, a, b);
        Swap(C4, a, b);
        Swap(C5, a, b);
        Swap(C6, a, b);
        Swap(C7, a, b);
    }

    private static void Swap<T>(T[] values, int a, int b) => (values[a], values[b]) = (values[b], values[a]);
}

/// <summary>A sort key column; implemented by structs so each sort is specialized.</summary>
public interface IKey
{
    static abstract int Compare(Table table, int a, int b);
}

/// <summary>C1 <c>uint64</c>.</summary>
public readonly struct KeyC1 : IKey
{
    public static int Compare(Table table, int a, int b) => table.C1[a].CompareTo(table.C1[b]);
}

/// <summary>C2 <c>string(8)</c>.</summary>
public readonly struct KeyC2 : IKey
{
    public static int Compare(Table table, int a, int b) => GermanString.Compare(table.C2[a], table.C2[b]);
}

/// <summary>C7 <c>string(50)</c>.</summary>
public readonly struct KeyC7 : IKey
{
    public static int Compare(Table table, int a, int b) => GermanString.Compare(table.C7[a], table.C7[b]);
}
