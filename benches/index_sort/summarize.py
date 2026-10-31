#!/usr/bin/env python3
"""Summarize index-sort harness reports without third-party packages."""

import argparse
import json
import math
from pathlib import Path
import re


def summarize_cpp(paths, method):
    """Pool Google Benchmark process repetitions, including between-run variance."""
    columns = {}
    for path in paths:
        if not path.exists():
            continue
        run = {}
        for benchmark in json.loads(path.read_text())["benchmarks"]:
            name = benchmark.get("run_name", "").split("/")
            statistic = benchmark.get("aggregate_name")
            if name[0] != method or statistic not in ("mean", "stddev"):
                continue
            scale = {"ns": 0.001, "us": 1, "ms": 1000, "s": 1_000_000}[benchmark["time_unit"]]
            stats = run.setdefault(name[1], {"repetitions": benchmark["repetitions"]})
            stats[statistic] = benchmark["cpu_time"] * scale
        for column, stats in run.items():
            columns.setdefault(column, []).append(stats)
    result = {}
    for column, runs in columns.items():
        count = sum(run["repetitions"] for run in runs)
        mean = sum(run["repetitions"] * run["mean"] for run in runs) / count
        variance = sum((run["repetitions"] - 1) * run.get("stddev", 0)**2
                       + run["repetitions"] * (run["mean"] - mean)**2 for run in runs)
        result[column] = {"time": mean, "stddev": math.sqrt(variance / (count - 1)) if count > 1 else 0,
                          "repetitions": count}
    return result


def summarize(directory):
    result = {"unit": "microseconds", "rust": {}, "gd_rs": {}, "csharp": {}, "kotlin": {}}
    for group, destination in [("sort_methods", "rust"), ("gd_rs", "gd_rs")]:
        for path in sorted((directory / "rust-criterion" / group).glob("*/*/new/estimates.json")):
            if path.parts[-4] == "append_order_view":
                continue
            estimates = json.loads(path.read_text())
            estimate = estimates.get("slope") or estimates["mean"]
            interval = estimate["confidence_interval"]
            result[destination].setdefault(path.parts[-4], {})[path.parts[-3]] = {
                "time": estimate["point_estimate"] / 1000,
                "error": (interval["upper_bound"] - interval["lower_bound"]) / 2000,
                "confidence": interval["confidence_level"],
            }
    reports = list((directory / "csharp-reports").glob("*-full-compressed.json"))
    reports += list((directory / "final/csharp-reports").glob("*-full-compressed.json"))
    reports += list((directory / "final/policy/csharp-reports").glob("*-full-compressed.json"))
    for report in reports:
        for benchmark in json.loads(report.read_text())["Benchmarks"]:
            parameters = dict(re.findall(r"(Column|Variant)=([^,& ]+)", benchmark["Parameters"]))
            statistics = benchmark["Statistics"]
            result["csharp"].setdefault(parameters["Variant"], {})[parameters["Column"]] = {
                "time": statistics["Mean"] / 1000,
                "stddev": statistics["StandardDeviation"] / 1000,
                "error": statistics["ConfidenceInterval"]["Margin"] / 1000,
                "confidence": 0.999,
                "memory": benchmark.get("Memory"),
            }
    reports = sorted((directory / "kotlin-reports").glob("*/main.json"))
    if reports:
        for benchmark in json.loads(reports[-1].read_text()):
            parameters, metric = benchmark["params"], benchmark["primaryMetric"]
            result["kotlin"].setdefault(parameters["variant"], {})[parameters["column"]] = {
                "time": metric["score"], "error": metric["scoreError"], "confidence": 0.999,
            }
    final = directory / "final"
    if (final / "rust-criterion").exists():
        result["gd_rs_before"] = result["gd_rs"]
        result["gd_rs"] = summarize(final)["gd_rs"]
    for destination, method in [("cpp_std_sort", "std_sort"), ("cpp_gd_key", "key"), ("cpp_soa_rows", "soa_rows")]:
        for reports in [[directory / "cpp.json", directory / "cpp-repeat.json"], [final / "cpp.json"]]:
            values = summarize_cpp(reports, method)
            if values:
                result[destination] = values
    for name in ["numeric", "long"]:
        report = final / f"kotlin-{name}.json"
        if report.exists():
            result[f"kotlin_confirmation_{name}"] = [
                {"column": b["params"]["column"], "variant": b["params"]["variant"],
                 "time": b["primaryMetric"]["score"], "error": b["primaryMetric"]["scoreError"]}
                for b in json.loads(report.read_text())
            ]
    policy = final / "policy"
    report = policy / "kotlin.json"
    if report.exists():
        result["kotlin"]["optimized"] = {
            b["params"]["column"]: {"time": b["primaryMetric"]["score"], "error": b["primaryMetric"]["scoreError"], "confidence": 0.999}
            for b in json.loads(report.read_text())
        }
    report = policy / "kotlin-short.json"
    if report.exists():
        result["kotlin_short_initial"] = result["kotlin"]["optimized"]["1"]
        metric = json.loads(report.read_text())[0]["primaryMetric"]
        result["kotlin"]["optimized"]["1"] = {"time": metric["score"], "error": metric["scoreError"], "confidence": 0.999}
    report = policy / "kotlin-reuse.json"
    if report.exists():
        for b in json.loads(report.read_text()):
            if b["params"]["column"] == "0":
                metric = b["primaryMetric"]
                result["kotlin"]["optimized"]["0"] = {"time": metric["score"], "error": metric["scoreError"], "confidence": 0.999}
    report = policy / "kotlin-control.json"
    if report.exists():
        result["kotlin_short_control"] = json.loads(report.read_text())
        for b in result["kotlin_short_control"]:
            if b["params"]["variant"] == "optimized":
                metric = b["primaryMetric"]
                result["kotlin"]["optimized"]["1"] = {"time": metric["score"], "error": metric["scoreError"], "confidence": 0.999}
    latest = directory / "latest_gd"
    values = summarize_cpp([latest / "key.json", latest / "key-repeat.json"], "key")
    if values:
        result["cpp_gd_key"] = values
    values = summarize_cpp([directory / "soa_cpp/soa.json", directory / "soa_cpp/soa-repeat.json"], "soa_rows")
    if values:
        result["cpp_soa_rows"] = values
    for path in sorted((latest / "rust-criterion/gd_rs/view_materialize").glob("*/new/estimates.json")):
        estimates = json.loads(path.read_text())
        estimate = estimates.get("slope") or estimates["mean"]
        interval = estimate["confidence_interval"]
        result["gd_rs"].setdefault("view_materialize", {})[path.parts[-3]] = {
            "time": estimate["point_estimate"] / 1000,
            "error": (interval["upper_bound"] - interval["lower_bound"]) / 2000,
            "confidence": interval["confidence_level"],
        }
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("directory", type=Path)
    args = parser.parse_args()
    result = summarize(args.directory)
    (args.directory / "summary.json").write_text(json.dumps(result, indent=2) + "\n")
    for name in ["cpp_gd_key", "cpp_std_sort", "cpp_soa_rows"]:
        if name in result:
            print(f"\n{name}: µs per operation (mean ± sample standard deviation)")
            for column, value in sorted(result[name].items()):
                print(f"{column}: {value['time']:.2f} ± {value['stddev']:.2f} ({value['repetitions']} repetitions)")
    for language in ["rust", "gd_rs", "csharp", "kotlin"]:
        variants = result[language]
        if not variants:
            continue
        print(f"\n{language}: µs per operation")
        print("| Column | " + " | ".join(variants) + " |")
        print("|---|" + "---:|" * len(variants))
        for column in map(str, range(7)):
            print("| " + column + " | " + " | ".join(
                f"{values[column]['time']:.2f}" if column in values else "—"
                for values in variants.values()
            ) + " |")


if __name__ == "__main__":
    main()
