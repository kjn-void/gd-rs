package indexsort

import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertTrue
import kotlin.test.assertFailsWith

class SortTest {
    private fun rowsOf(table: Table) = List(table.size) {
        listOf(table.c1[it], table.c2.get(it), table.c3[it], table.c4[it], table.c5[it], table.c6[it], table.c7.get(it)).joinToString(" ")
    }

    /** Sorts on every column and checks the order and that no row was split or lost. */
    private fun checkEveryColumn(source: Table) {
        for (key in Key.entries) {
            for (variant in listOf("index", "reuse", "validated", "cached", "optimized")) {
                var target = Table(source.size)
                val workspace = SortWorkspace(source.size)
                repeat(2) {
                    target.appendFrom(source)
                    target = when (variant) {
                        "index" -> indexSort(target, key.comparator)
                        "optimized" -> optimizedSort(target, key, workspace)
                        else -> workspace.sort(target, key, variant)
                    }
                    requireSorted(target, key.comparator)
                    assertEquals(rowsOf(source).sorted(), rowsOf(target).sorted(), "rows were split or lost, $key, $variant")
                }
            }
        }
    }

    @Test
    fun indexSortOrdersEveryColumn() {
        checkEveryColumn(Table.fromSource(generateRows(ROW_COUNT)))
    }

    @Test
    fun indexSortHandlesDuplicatesAndShortTables() {
        // C5 and C6 hold only 1,000 distinct values, and C1 is reduced to three.
        val rows = generateRows(1000).map { SourceRow(it.c1 % 3, it.c2, it.c3, it.c4, it.c5, it.c6, it.c7) }
        for (count in listOf(0, 1, 2, 3, 16, 17, 100, 1000)) {
            checkEveryColumn(Table.fromSource(rows.take(count)))
        }
    }

    @Test
    fun reusableSortsRejectNullKeys() {
        for (key in listOf(Key.C4, Key.C5, Key.C6)) {
            for (variant in listOf("reuse", "validated", "cached")) {
                val table = Table.fromSource(generateRows(2))
                when (key) {
                    Key.C4 -> table.c4Valid[0] = false
                    Key.C5 -> table.c5Valid[0] = false
                    Key.C6 -> table.c6Valid[0] = false
                    else -> error("not nullable")
                }
                assertFailsWith<IllegalStateException> { SortWorkspace(2).sort(table, key, variant) }
            }
        }
    }

    @Test
    fun cachedNumbersHandleSignedUnsignedAndSpecialFloats() {
        val table = Table.fromSource(generateRows(100))
        val doubles = doubleArrayOf(Double.NaN, Double.NEGATIVE_INFINITY, -0.0, 0.0, Double.POSITIVE_INFINITY, -2.0)
        for (row in 0 until table.size) {
            table.c1[row] = if (row % 2 == 0) Long.MIN_VALUE + row else row.toLong()
            table.c3[row] = doubles[row % doubles.size]
            table.c4[row] = table.c3[row]
            table.c5[row] = if (row % 2 == 0) Int.MIN_VALUE + row else row
            table.c6[row] = if (row % 2 == 0) Int.MIN_VALUE + row else row
        }
        checkEveryColumn(table)
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
    fun rowsMatchTheBenchmarkDistribution() {
        for (row in generateRows(ROW_COUNT)) {
            assertEquals(8, row.c2.length)
            assertEquals(45, row.c7.length)
            assertTrue(row.c7.startsWith("01234567890123456789"))
            assertTrue((row.c2 + row.c7.substring(20)).all { it in 'a'..'z' })
            assertTrue(row.c4 >= 0.0 && row.c4 < 1_000.0)
            assertTrue(row.c5 in 0 until 1000 && row.c6 in 0 until 1000)
        }
    }
}
