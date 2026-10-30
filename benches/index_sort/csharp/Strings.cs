using System.Buffers.Binary;
using System.Text;

namespace IndexSort;

/// <summary>
/// German (Umbra) string in the layout of <c>strumbra::SharedString</c>: length, a
/// 4-byte prefix, and either the next 8 bytes inline (up to 12 bytes in total) or a
/// shared <c>byte[]</c> holding the whole string. The struct is 24 bytes, because a
/// managed reference cannot overlap the inline bytes.
/// </summary>
/// <remarks>
/// The prefix and inline bytes are packed big-endian and zero padded, so unsigned
/// integer comparison, followed by length, gives byte-wise lexicographic order.
/// </remarks>
public readonly struct GermanString
{
    private const int InlineLength = 12;

    private readonly uint _length;
    private readonly uint _prefix;
    private readonly ulong _inline;
    private readonly byte[]? _heap;

    private GermanString(byte[] bytes)
    {
        _length = (uint)bytes.Length;
        Span<byte> padded = stackalloc byte[InlineLength];
        bytes.AsSpan(0, Math.Min(bytes.Length, InlineLength)).CopyTo(padded);
        _prefix = BinaryPrimitives.ReadUInt32BigEndian(padded);
        if (bytes.Length <= InlineLength)
        {
            _inline = BinaryPrimitives.ReadUInt64BigEndian(padded[4..]);
        }
        else
        {
            _heap = bytes;
        }
    }

    public string Value => Encoding.UTF8.GetString(Bytes());

    public static GermanString Create(string value) => new(Encoding.UTF8.GetBytes(value));

    public static int Compare(in GermanString a, in GermanString b)
    {
        if (a._prefix != b._prefix)
        {
            return a._prefix < b._prefix ? -1 : 1;
        }
        if (a._heap is null && b._heap is null)
        {
            return a._inline != b._inline ? (a._inline < b._inline ? -1 : 1) : a._length.CompareTo(b._length);
        }
        return a.Bytes().AsSpan(4).SequenceCompareTo(b.Bytes().AsSpan(4));
    }

    private byte[] Bytes()
    {
        if (_heap is not null)
        {
            return _heap;
        }
        Span<byte> padded = stackalloc byte[InlineLength];
        BinaryPrimitives.WriteUInt32BigEndian(padded, _prefix);
        BinaryPrimitives.WriteUInt64BigEndian(padded[4..], _inline);
        return padded[..(int)_length].ToArray();
    }
}
