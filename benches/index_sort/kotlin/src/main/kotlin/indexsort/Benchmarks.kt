// Kotlin counterpart of ../cpp/sort_methods_benchmark.cpp.
//
// A 10,000-row table (or INDEX_SORT_ROWS) with every column set is sorted on each of its seven columns
// with the port's index sort. Each operation clears the target, appends the source
// rows, and sorts, like the C++ cases.

package indexsort

import java.util.concurrent.TimeUnit
import org.openjdk.jmh.annotations.Benchmark
import org.openjdk.jmh.annotations.BenchmarkMode
import org.openjdk.jmh.annotations.Fork
import org.openjdk.jmh.annotations.Level
import org.openjdk.jmh.annotations.Measurement
import org.openjdk.jmh.annotations.Mode
import org.openjdk.jmh.annotations.OutputTimeUnit
import org.openjdk.jmh.annotations.Param
import org.openjdk.jmh.annotations.Scope
import org.openjdk.jmh.annotations.Setup
import org.openjdk.jmh.annotations.State
import org.openjdk.jmh.annotations.TearDown
import org.openjdk.jmh.annotations.Warmup

@State(Scope.Benchmark)
@BenchmarkMode(Mode.AverageTime)
@OutputTimeUnit(TimeUnit.MICROSECONDS)
@Warmup(iterations = 5, time = 1)
@Measurement(iterations = 5, time = 1)
@Fork(1)
open class SortMethods {
    /** The sort column, by C++ column number. */
    @Param("0", "1", "2", "3", "4", "5", "6")
    var column = 0

    @Param("index", "reuse", "validated", "cached", "optimized")
    var variant = "index"

    private lateinit var workspace: SortWorkspace
    private lateinit var key: Key
    private lateinit var source: Table
    private lateinit var target: Table

    @Setup(Level.Trial)
    fun setUp() {
        key = Key.entries.first { it.column == column }
        val rows = benchmarkRowCount()
        source = Table.fromSource(generateRows(rows))
        target = Table(rows)
        workspace = SortWorkspace(rows)
    }

    @TearDown(Level.Trial)
    fun tearDown() = requireSorted(target, key.comparator)

    @Benchmark
    fun index(): Int {
        target.appendFrom(source)
        target = when (variant) {
            "index" -> indexSort(target, key.comparator)
            "optimized" -> optimizedSort(target, key, workspace)
            else -> workspace.sort(target, key, variant)
        }
        return target.size
    }
}
