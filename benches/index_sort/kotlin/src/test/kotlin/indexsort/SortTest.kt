package indexsort

import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertTrue

class SortTest {
    private val rows = generateRows(ROW_COUNT)

    @Test
    fun bothSortsOrderEveryKeyColumn() {
        val source = Table.fromSource(rows)
        for (key in Key.entries) {
            val table = Table(ROW_COUNT)
            table.appendFrom(source)
            physicalSort(table, key.comparator)
            requireSorted(table, key.comparator)
            requireSorted(indexSort(source, key.comparator), key.comparator)
        }
    }

    @Test
    fun bothSortsHandleDuplicatesAndShortTables() {
        val duplicates = rows.map { SourceRow(it.c1 % 3, it.c2, it.c3, it.c7) }
        for (count in listOf(0, 1, 2, 3, 16, 17, 100, ROW_COUNT)) {
            val source = Table.fromSource(duplicates.take(count))
            val table = Table(count)
            table.appendFrom(source)
            physicalSort(table, Key.C1.comparator)
            requireSorted(table, Key.C1.comparator)
            val index = indexSort(source, Key.C1.comparator)
            requireSorted(index, Key.C1.comparator)
            assertEquals(count, index.size)
        }
    }

    @Test
    fun bothSortsProduceTheSameRows() {
        val source = Table.fromSource(rows)
        fun rowsOf(table: Table) = List(table.size) { Triple(table.c1[it], table.c2.get(it), table.c7.get(it)) }

        val physical = Table(ROW_COUNT).apply { appendFrom(source) }
        physicalSort(physical, Key.C7.comparator)
        val index = indexSort(source, Key.C7.comparator)
        assertEquals(rowsOf(physical), rowsOf(index))

        val order = compareBy<Triple<Long, String, String>>({ it.first }, { it.second }, { it.third })
        assertEquals(rowsOf(source).sortedWith(order), rowsOf(index).sortedWith(order), "rows were split or lost")
    }

    @Test
    fun germanStringsOrderLikeBytes() {
        val values = listOf("", "a", "ab", "ab\u0000", "abc", "abcd", "abcdefghijkl", "abcdefghijklm", "b", "zz")
        val column = GermanStringColumn(values.size)
        values.forEachIndexed { row, value -> column.set(row, value) }
        for (a in values.indices) {
            assertEquals(values[a], column.get(a))
            for (b in values.indices) {
                assertEquals(values[a].compareTo(values[b]).coerceIn(-1, 1), column.compare(a, b).coerceIn(-1, 1), "${values[a]} vs ${values[b]}")
            }
        }
    }

    @Test
    fun stringsMatchTheBenchmarkDistribution() {
        for (row in rows) {
            assertEquals(8, row.c2.length)
            assertEquals(45, row.c7.length)
            assertTrue(row.c7.startsWith("01234567890123456789"))
            assertTrue((row.c2 + row.c7.substring(20)).all { it in 'a'..'z' })
        }
    }
}
