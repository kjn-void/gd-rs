#ifndef GD_RS_BENCHES_CPP_REFERENCE_CACHE_LATENCY_PERF_CONTROL_H_
#define GD_RS_BENCHES_CPP_REFERENCE_CACHE_LATENCY_PERF_CONTROL_H_

#include <fstream>

// Optional perf-stat control. Setup, warmup, and verification are not counted.
class PerfControl {
 public:
  void Start();
  void Stop();

 private:
  void Command(const char* text);

  std::ofstream control_;
  std::ifstream acknowledgement_;
};

#endif  // GD_RS_BENCHES_CPP_REFERENCE_CACHE_LATENCY_PERF_CONTROL_H_
