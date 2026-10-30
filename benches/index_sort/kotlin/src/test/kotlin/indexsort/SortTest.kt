package indexsort

import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertTrue

class SortTest {
    private val rows = generateRows(ROW_COUNT)

    @Test
    fun bothSortsOrderEveryKeyColumn() {
        val source = Table.fromSource(rows)
        for (key in listOf(Key.C1, Key.C2, Key.C7)) {
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
    fun threeMethodsOrderEveryColumnTheSameWay() {
        val source = Table.fromSource(generateAllColumns(SORT_METHODS_ROW_COUNT))
        for (key in Key.entries) {
            val physical = Table(source.size).apply { appendFrom(source) }
            physicalSort(physical, key.comparator)
            val index = indexSort(source, key.comparator)
            val keyed = keySort(source, key)
            for (table in listOf(physical, index, keyed)) requireSorted(table, key.comparator)
            assertEquals(allColumns(physical), allColumns(index), "index, $key")
            assertEquals(allColumns(physical), allColumns(keyed), "key, $key")
            assertEquals(allColumns(source).sorted(), allColumns(keyed).sorted(), "rows were split or lost, $key")
        }
    }

    @Test
    fun keySortHandlesDuplicatesAndShortTables() {
        val duplicates = generateAllColumns(ROW_COUNT).map { SourceRow(it.c1 % 3, it.c2, it.c3, it.c7, it.c4, it.c5, it.c6) }
        for (count in listOf(0, 1, 2, 3, 16, 17, 100, ROW_COUNT)) {
            for (key in listOf(Key.C1, Key.C6)) {
                val sorted = keySort(Table.fromSource(duplicates.take(count)), key)
                requireSorted(sorted, key.comparator)
                assertEquals(count, sorted.size)
            }
        }
    }

    private fun allColumns(table: Table) = List(table.size) {
        listOf(table.c1[it], table.c2.get(it), table.c3[it], table.c4[it], table.c5[it], table.c6[it], table.c7.get(it)).joinToString(" ")
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
