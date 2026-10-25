#include "perf_control.h"

#include <cstdlib>
#include <stdexcept>
#include <string>

void PerfControl::Start() {
  const char* control_path = std::getenv("CACHE_PERF_CTL");

  if (control_path != nullptr) {
    const char* acknowledgement_path = std::getenv("CACHE_PERF_ACK");

    if (acknowledgement_path == nullptr) {
      throw std::runtime_error("CACHE_PERF_ACK is missing");
    }

    control_.open(control_path);
    acknowledgement_.open(acknowledgement_path);

    Command("disable");
    Command("enable");
  }
}

void PerfControl::Stop() {
  if (control_.is_open()) {
    Command("disable");
  }
}

void PerfControl::Command(const char* text) {
  control_ << text << '\n' << std::flush;

  std::string reply;

  if (!std::getline(acknowledgement_, reply)) {
    throw std::runtime_error("perf did not acknowledge command");
  }
}
