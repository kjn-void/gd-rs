package indexsort

import java.util.Arrays

/**
 * German (Umbra) strings in the layout of `strumbra::SharedString`, split across
 * parallel arrays: length, a 4-byte prefix, and either the next 8 bytes inline (up
 * to 12 bytes in total) or a shared `ByteArray` holding the whole string.
 *
 * The prefix and inline bytes are packed big-endian and zero padded, so unsigned
 * integer comparison, followed by length, gives byte-wise lexicographic order.
 */
class GermanStringColumn(val capacity: Int) {
    private val lengths = IntArray(capacity)
    private val prefixes = IntArray(capacity)
    private val inline = LongArray(capacity)
    private val heap = arrayOfNulls<ByteArray>(capacity)

    fun set(row: Int, value: String) {
        val bytes = value.toByteArray(Charsets.UTF_8)
        lengths[row] = bytes.size
        prefixes[row] = pack(bytes, 0, 4).toInt()
        if (bytes.size <= INLINE_LENGTH) {
            inline[row] = pack(bytes, 4, 8)
            heap[row] = null
        } else {
            inline[row] = 0
            heap[row] = bytes
        }
    }

    fun get(row: Int): String = String(bytes(row), Charsets.UTF_8)

    fun compare(a: Int, b: Int): Int {
        if (prefixes[a] != prefixes[b]) return Integer.compareUnsigned(prefixes[a], prefixes[b])
        val heapA = heap[a]
        val heapB = heap[b]
        if (heapA == null && heapB == null) {
            val order = java.lang.Long.compareUnsigned(inline[a], inline[b])
            return if (order != 0) order else lengths[a].compareTo(lengths[b])
        }
        val bytesA = heapA ?: bytes(a)
        val bytesB = heapB ?: bytes(b)
        return Arrays.compareUnsigned(bytesA, 4, bytesA.size, bytesB, 4, bytesB.size)
    }

    fun swap(a: Int, b: Int) {
        val length = lengths[a]
        lengths[a] = lengths[b]
        lengths[b] = length
        val prefix = prefixes[a]
        prefixes[a] = prefixes[b]
        prefixes[b] = prefix
        val inlineBytes = inline[a]
        inline[a] = inline[b]
        inline[b] = inlineBytes
        val heapBytes = heap[a]
        heap[a] = heap[b]
        heap[b] = heapBytes
    }

    /** Replaces rows `0 until count` with `source`'s rows. Heap text is shared, not copied. */
    fun copyFrom(source: GermanStringColumn, count: Int) {
        System.arraycopy(source.lengths, 0, lengths, 0, count)
        System.arraycopy(source.prefixes, 0, prefixes, 0, count)
        System.arraycopy(source.inline, 0, inline, 0, count)
        System.arraycopy(source.heap, 0, heap, 0, count)
    }

    /** Sets row `k` to `source`'s row `index[k]` for every `k` in `index`. Heap text is shared. */
    fun gatherFrom(source: GermanStringColumn, index: IntArray) {
        for (position in index.indices) {
            val row = index[position]
            lengths[position] = source.lengths[row]
            prefixes[position] = source.prefixes[row]
            inline[position] = source.inline[row]
            heap[position] = source.heap[row]
        }
    }

    private fun bytes(row: Int): ByteArray {
        heap[row]?.let { return it }
        val bytes = ByteArray(lengths[row])
        for (index in bytes.indices) {
            bytes[index] = if (index < 4) {
                (prefixes[row] ushr (24 - 8 * index)).toByte()
            } else {
                (inline[row] ushr (56 - 8 * (index - 4))).toByte()
            }
        }
        return bytes
    }

    private companion object {
        const val INLINE_LENGTH = 12

        /** Packs up to `width` bytes from `from` big-endian, zero padded. */
        fun pack(bytes: ByteArray, from: Int, width: Int): Long {
            var packed = 0L
            for (index in 0 until width) {
                val byte = bytes.getOrElse(from + index) { 0 }.toLong() and 0xff
                packed = (packed shl 8) or byte
            }
            return packed
        }
    }
}
