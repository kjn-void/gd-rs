// Kotlin counterpart of ../cpp/index_sort_benchmark.cpp.
//
// Each operation clears the target, appends the source rows, and sorts on one
// column, like the C++ cases.

package indexsort

import java.util.concurrent.TimeUnit
import org.openjdk.jmh.annotations.Benchmark
import org.openjdk.jmh.annotations.BenchmarkMode
import org.openjdk.jmh.annotations.Fork
import org.openjdk.jmh.annotations.Level
import org.openjdk.jmh.annotations.Measurement
import org.openjdk.jmh.annotations.Mode
import org.openjdk.jmh.annotations.OutputTimeUnit
import org.openjdk.jmh.annotations.Scope
import org.openjdk.jmh.annotations.Setup
import org.openjdk.jmh.annotations.State
import org.openjdk.jmh.annotations.TearDown
import org.openjdk.jmh.annotations.Warmup

/** Source and target tables plus the key of the last sort, checked after measuring. */
@State(Scope.Benchmark)
abstract class Fixture(private val rows: Int) {
    private lateinit var source: Table
    private lateinit var target: Table
    private var lastKey: Key? = null

    @Setup(Level.Trial)
    fun setUp() {
        source = Table.fromSource(generateRows(rows))
        target = Table(rows)
    }

    @TearDown(Level.Trial)
    fun tearDown() {
        lastKey?.let { requireSorted(target, it.comparator) }
    }

    protected fun appendOnly(): Int {
        target.appendFrom(source)
        return target.size
    }

    protected fun physical(key: Key): Int {
        target.appendFrom(source)
        physicalSort(target, key.comparator)
        lastKey = key
        return target.size
    }

    protected fun index(key: Key): Int {
        target.appendFrom(source)
        indexSort(target, key.comparator)
        lastKey = key
        return target.size
    }
}

@BenchmarkMode(Mode.AverageTime)
@OutputTimeUnit(TimeUnit.MICROSECONDS)
@Warmup(iterations = 5, time = 1)
@Measurement(iterations = 5, time = 1)
@Fork(1)
open class ThousandRows : Fixture(ROW_COUNT) {
    @Benchmark fun sort00AppendOnly() = appendOnly()
    @Benchmark fun sort01PhysicalUint64() = physical(Key.C1)
    @Benchmark fun sort02PhysicalString8() = physical(Key.C2)
    @Benchmark fun sort03PhysicalString50() = physical(Key.C7)
    @Benchmark fun sort04IndexUint64() = index(Key.C1)
    @Benchmark fun sort05IndexString8() = index(Key.C2)
    @Benchmark fun sort06IndexString50() = index(Key.C7)
}

@BenchmarkMode(Mode.AverageTime)
@OutputTimeUnit(TimeUnit.MILLISECONDS)
@Warmup(iterations = 3, time = 2)
@Measurement(iterations = 5, time = 2)
@Fork(1)
open class MillionRows : Fixture(MILLION_ROWS) {
    @Benchmark fun sort00AppendOnly() = appendOnly()
    @Benchmark fun sort01PhysicalUint64() = physical(Key.C1)
    @Benchmark fun sort02PhysicalString8() = physical(Key.C2)
    @Benchmark fun sort03PhysicalString50() = physical(Key.C7)
    @Benchmark fun sort04IndexUint64() = index(Key.C1)
    @Benchmark fun sort05IndexString8() = index(Key.C2)
    @Benchmark fun sort06IndexString50() = index(Key.C7)
}
