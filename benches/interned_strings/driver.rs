//! Uninstrumented timing executable; see `compare.py` for the matched C++ run.
mod workload;

fn main() {
    workload::run(|| 0);
}
