namespace IndexSort;

/// <summary>
/// Struct of arrays: one array per column, preallocated to the row capacity. C2 and C7
/// hold German strings; C4 to C6 are nullable, like GD's.
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
            table.C4[row] = rows[row].C4;
            table.C5[row] = rows[row].C5;
            table.C6[row] = rows[row].C6;
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

    /// <summary>
    /// Builds a new table whose row <c>k</c> is this table's row <c>index[k]</c>, one
    /// column at a time. <paramref name="index"/> must be a permutation of the rows.
    /// </summary>
    public Table Gather(int[] index)
    {
        var sorted = new Table(C1.Length);
        GatherInto(index, sorted);
        return sorted;
    }

    public void GatherInto(int[] index, Table sorted)
    {
        if (ReferenceEquals(this, sorted)) throw new ArgumentException("gather buffers must differ");
        Gather(C1, sorted.C1, index);
        Gather(C2, sorted.C2, index);
        Gather(C3, sorted.C3, index);
        Gather(C4, sorted.C4, index);
        Gather(C5, sorted.C5, index);
        Gather(C6, sorted.C6, index);
        Gather(C7, sorted.C7, index);
        sorted.Size = Size;
    }

    private static void Gather<T>(T[] from, T[] to, int[] index)
    {
        for (var position = 0; position < index.Length; position++)
        {
            to[position] = from[index[position]];
        }
    }
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

/// <summary>C3 <c>double</c>.</summary>
public readonly struct KeyC3 : IKey
{
    public static int Compare(Table table, int a, int b) => table.C3[a].CompareTo(table.C3[b]);
}

/// <summary>C4 <c>double</c>, nullable; like GD's sorts, key values must be set.</summary>
public readonly struct KeyC4 : IKey
{
    public static int Compare(Table table, int a, int b) => Key.Set(table.C4[a]).CompareTo(Key.Set(table.C4[b]));
}

/// <summary>C5 <c>int32</c>, nullable; key values must be set.</summary>
public readonly struct KeyC5 : IKey
{
    public static int Compare(Table table, int a, int b) => Key.Set(table.C5[a]).CompareTo(Key.Set(table.C5[b]));
}

/// <summary>C6 <c>uint32</c>, nullable; key values must be set.</summary>
public readonly struct KeyC6 : IKey
{
    public static int Compare(Table table, int a, int b) => Key.Set(table.C6[a]).CompareTo(Key.Set(table.C6[b]));
}

/// <summary>C7 <c>string(50)</c>.</summary>
public readonly struct KeyC7 : IKey
{
    public static int Compare(Table table, int a, int b) => GermanString.Compare(table.C7[a], table.C7[b]);
}

internal static class Key
{
    /// <summary>The value of a nullable sort key, which must be set.</summary>
    public static T Set<T>(T? value)
        where T : struct
        => value ?? throw new InvalidOperationException("sort key is null");
}
