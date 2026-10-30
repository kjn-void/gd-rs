// Kotlin counterpart of ../cpp/index_sort_benchmark.cpp.
//
// The six sort cases are not ported yet; this class only establishes the
// kotlinx-benchmark (JMH) harness.

package indexsort

import kotlinx.benchmark.Benchmark
import kotlinx.benchmark.Scope
import kotlinx.benchmark.State

@State(Scope.Benchmark)
class IndexSortBenchmark {
    @Benchmark
    fun harness(): Int = 0
}
