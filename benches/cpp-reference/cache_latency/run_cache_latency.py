#!/usr/bin/env python3
"""Measure one generic C++ executable on RK3588 A55 and A76 cores."""

import argparse
import datetime
import hashlib
import json
import os
from pathlib import Path
import random
import statistics
import subprocess


EVENTS = [
    "cpu_cycles",
    "inst_retired",
    "l1d_cache_refill",
    "l2d_cache_refill",
    "br_mis_pred_retired",
    "stall_backend",
]


def save_json(path, value):
    path.write_text(json.dumps(value, indent=2) + "\n")


def read_cache(core):
    folder = Path(f"/sys/devices/system/cpu/cpu{core}/cache")
    keys = ["level", "type", "size", "coherency_line_size", "shared_cpu_list"]

    return [
        {key: (index / key).read_text().strip() for key in keys}
        for index in sorted(folder.glob("index*"))
    ]


def read_counts(path, event_names):
    counts = {}

    for line in path.read_text().splitlines():
        columns = line.split(",")

        if len(columns) <= 4 or not columns[0].strip().isdigit():
            continue

        if float(columns[4]) < 99.0:
            raise RuntimeError(f"Counter multiplexing: {line}")

        counts[columns[2]] = int(columns[0])

    if any(name not in counts for name in event_names):
        raise RuntimeError(f"Missing counter in {path}:\n{path.read_text()}")

    return dict(zip(EVENTS, (counts[name] for name in event_names)))


def run_trial(binary, output, trial, number, seconds):
    repetition, label, core, pmu, kib, kernel, seed = trial
    tag = f"{number:03d}-{label}-{kernel}-{kib}-{seed}"

    control = output / f"{tag}.ctl"
    acknowledgement = output / f"{tag}.ack"
    counters = output / f"{tag}.csv"

    os.mkfifo(control)
    os.mkfifo(acknowledgement)

    environment = dict(
        os.environ,
        CACHE_PERF_CTL=str(control),
        CACHE_PERF_ACK=str(acknowledgement),
    )
    names = [f"{pmu}/{event}/u" for event in EVENTS]

    command = [
        "perf", "stat", "--delay=-1",
        f"--control=fifo:{control},{acknowledgement}",
        "-x,", "-o", str(counters), "-e", ",".join(names), "--",
        "taskset", "-c", str(core), str(binary), kernel,
        str(kib), str(seconds), str(seed),
    ]

    try:
        process = subprocess.run(
            command, env=environment, text=True, capture_output=True,
            timeout=30, check=True,
        )
    finally:
        control.unlink()
        acknowledgement.unlink()

    result = json.loads(process.stdout)
    result.update(cpu=label, core=core, repetition=repetition)
    result["counters"] = read_counts(counters, names)

    for event, count in result["counters"].items():
        result[event + "_per_iteration"] = count / result["iterations"]

    result["ipc"] = (
        result["counters"]["inst_retired"] / result["counters"]["cpu_cycles"]
    )

    return result


def summarize(results, cores, arguments):
    summary = []
    metrics = ["ns_per_iteration", "ipc"] + [
        event + "_per_iteration" for event in EVENTS
    ]

    for label, _, _ in cores:
        for kernel in arguments.kernels:
            for kib in arguments.sizes:
                samples = [
                    row for row in results
                    if row["cpu"] == label and row["kernel"] == kernel
                    and row["cold_kib"] == kib
                ]
                medians = {
                    metric: statistics.median(row[metric] for row in samples)
                    for metric in metrics
                }
                summary.append(dict(
                    cpu=label, kernel=kernel, cold_kib=kib, samples=len(samples),
                    **medians,
                    min_ns=min(row["ns_per_iteration"] for row in samples),
                    max_ns=max(row["ns_per_iteration"] for row in samples),
                ))

    return summary


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--a55-core", type=int, default=3)
    parser.add_argument("--a76-core", type=int, default=4)
    parser.add_argument("--reps", type=int, default=3)
    parser.add_argument("--seconds", type=float, default=0.25)
    parser.add_argument("--sizes", nargs="+", type=int, default=[8, 80])
    parser.add_argument("--seeds", nargs="+", type=int, default=[70274])
    parser.add_argument(
        "--kernels", nargs="+",
        default=["work0", "work1", "work2", "work4", "serial"],
    )
    arguments = parser.parse_args()

    binary = arguments.binary.resolve()
    output = arguments.output.resolve()
    output.mkdir(parents=True, exist_ok=True)

    if (output / "metadata.json").exists():
        raise RuntimeError("Choose a fresh output directory")

    cores = [
        ("a55", arguments.a55_core, "armv8_cortex_a55"),
        ("a76", arguments.a76_core, "armv8_cortex_a76"),
    ]
    trials = [
        (rep, label, core, pmu, kib, kernel, seed)
        for rep in range(arguments.reps)
        for label, core, pmu in cores
        for kib in arguments.sizes
        for kernel in arguments.kernels
        for seed in arguments.seeds
    ]

    random.Random(271824 + arguments.reps).shuffle(trials)
    metadata = dict(
        timestamp_utc=datetime.datetime.now(datetime.timezone.utc).isoformat(),
        uname=subprocess.check_output(["uname", "-a"], text=True).strip(),
        compiler=subprocess.check_output(
            ["g++", "--version"], text=True,
        ).splitlines()[0],
        flags=(
            "-std=c++20 -O3 -flto -Wall -Wextra; "
            "L1_CACHE_BYTES or L1_CACHE_SHIFT from Linux; no native flags"
        ),
        binary_sha256=hashlib.sha256(binary.read_bytes()).hexdigest(),
        source_sha256=hashlib.sha256(
            (Path(__file__).with_name("cache_latency.cpp")).read_bytes()
        ).hexdigest(),
        source_sha256_by_file={
            name: hashlib.sha256(
                Path(__file__).with_name(name).read_bytes()
            ).hexdigest()
            for name in ("cache_latency.cpp", "perf_control.cpp", "perf_control.h")
        },
        cores=cores, perf_events=EVENTS, repetitions=arguments.reps,
        seconds=arguments.seconds, seeds=arguments.seeds, sizes=arguments.sizes,
        kernels=arguments.kernels, warmup_seconds=0.1,
        caches={label: read_cache(core) for label, core, _ in cores},
    )
    save_json(output / "metadata.json", metadata)

    verification = subprocess.check_output([str(binary), "--verify"], text=True)
    (output / "verification.txt").write_text(verification)
    results = []

    for number, trial in enumerate(trials):
        result = run_trial(binary, output, trial, number, arguments.seconds)
        results.append(result)
        save_json(output / "results.json", results)

        if number % 12 == 0:
            print(
                f"{number + 1}/{len(trials)} {result['cpu']} "
                f"{result['kernel']} {result['cold_kib']} KiB: "
                f"{result['cpu_cycles_per_iteration']:.3f} cycles, "
                f"{result['l1d_cache_refill_per_iteration']:.4f} L1 refills",
                flush=True,
            )

    summary = summarize(results, cores, arguments)
    save_json(output / "summary.json", summary)

    for row in summary:
        print(json.dumps(row), flush=True)


if __name__ == "__main__":
    main()
