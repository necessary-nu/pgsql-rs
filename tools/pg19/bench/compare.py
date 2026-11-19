"""Compare the two in-process PG19 benchmark executables on one logical CPU."""
import argparse
import csv
import hashlib
import json
import os
import platform
import random
import statistics
import subprocess
import tempfile
import time
from pathlib import Path

REPEATS = {"simple": 300_000, "join": 20_000, "corpus": 1}
MODES = ("parse", "json")


def cpu_times():
    result = {}
    for line in Path("/proc/stat").read_text().splitlines():
        label, *fields = line.split()
        if label.startswith("cpu") and label[3:].isdigit():
            ticks = list(map(int, fields))
            result[int(label[3:])] = (ticks[3] + ticks[4], sum(ticks[:8]))
    return result


def choose_cpu():
    before = cpu_times()
    time.sleep(1.5)
    rates = {}
    for cpu, (idle, total) in cpu_times().items():
        if cpu in before:
            rates[cpu] = (idle - before[cpu][0]) / max(1, total - before[cpu][1])

    def score(cpu):
        topology = Path(f"/sys/devices/system/cpu/cpu{cpu}/topology/thread_siblings_list")
        siblings = {cpu}
        if topology.exists():
            for part in topology.read_text().strip().split(","):
                ends = [int(value) for value in part.split("-")]
                siblings.update(range(ends[0], ends[-1] + 1))
        return min(rates.get(s, 0) for s in siblings), rates[cpu], cpu != 0, -cpu

    return max(os.sched_getaffinity(0), key=score)


def command(cpu, binary, mode, path, repeats):
    return ["taskset", "-c", str(cpu), binary, mode, path, str(repeats)]


def verify(cpu, binaries, path):
    rows = 0
    digest = hashlib.sha256()
    with tempfile.TemporaryFile() as c, tempfile.TemporaryFile() as rust:
        for language, dest in [("C", c), ("Rust", rust)]:
            subprocess.run(command(cpu, binaries[language], "verify", path, 1), stdout=dest, check=True)
            dest.seek(0)
        for rows, (left, right) in enumerate(zip(c, rust, strict=True), 1):
            if left != right:
                raise ValueError(f"{path.name}: JSON bytes differ at input {rows}")
            digest.update(left)
    data = path.read_bytes()
    if rows == 0 or rows != data.count(b"\0"):
        raise ValueError(f"{path.name}: expected one tree per input, got {rows}")
    return {"rows": rows, "input_sha256": hashlib.sha256(data).hexdigest(), "json_sha256": digest.hexdigest()}


def measure(work, cpu, binaries, verified, rounds):
    times = {}
    workloads = [(name, mode) for name in REPEATS for mode in MODES]
    rng = random.Random(20261119)
    with (work / "samples.csv").open("w", newline="") as output:
        writer = csv.writer(output)
        writer.writerow(["round", "workload", "operation", "implementation", "total_ns", "calls", "statements", "ns_per_call"])
        for round_index in range(rounds):
            rng.shuffle(workloads)
            for name, mode in workloads:
                order = list(binaries)
                if (round_index + list(REPEATS).index(name) + (mode == "json")) % 2:
                    order.reverse()
                statements = []
                for lang in order:
                    args = command(cpu, binaries[lang], mode, work / f"{name}.sql0", REPEATS[name])
                    ns, calls, count = map(int, subprocess.check_output(args, text=True).split())
                    if calls != verified[name]["rows"] * REPEATS[name] or ns <= 0:
                        raise ValueError(f"{name} {mode} {lang}: invalid measurement {ns} ns for {calls} calls")
                    statements.append(count)
                    times.setdefault((name, mode, lang), []).append(ns / calls / 1000)
                    writer.writerow([round_index + 1, name, mode, lang, ns, calls, count, ns / calls])
                if statements[0] != statements[1]:
                    raise ValueError(f"{name} {mode}: statement counts differ {statements}")
                output.flush()
            print(f"Finished paired round {round_index + 1}/{rounds}", flush=True)
    return times


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("work", type=Path)
    parser.add_argument("rust", type=Path)
    parser.add_argument("--runs", type=int, default=7)
    parser.add_argument("--cpu", type=int)
    args = parser.parse_args()
    if args.runs < 1:
        parser.error("expected at least one paired round")

    work = args.work.resolve()
    binaries = {"C": work / "bench-c", "Rust": args.rust.resolve()}
    cpu = choose_cpu() if args.cpu is None else args.cpu
    print(f"CPU {cpu}, verifying outputs before timing.", flush=True)
    verified = {name: verify(cpu, binaries, work / f"{name}.sql0") for name in REPEATS}
    info = {"cpu": cpu, "runs": args.runs, "platform": platform.platform(), "verification": verified}
    info["binary_sha256"] = {lang: hashlib.sha256(path.read_bytes()).hexdigest() for lang, path in binaries.items()}
    (work / "machine-and-verification.json").write_text(json.dumps(info, indent=2) + "\n")
    times = measure(work, cpu, binaries, verified, args.runs)
    summary = []
    for name in REPEATS:
        for mode in MODES:
            item = {"workload": name, "operation": mode}
            for lang in binaries:
                samples = times[name, mode, lang]
                item[lang] = {"median_us": statistics.median(samples), "min_us": min(samples), "max_us": max(samples)}
            ratio = item["Rust"]["median_us"] / item["C"]["median_us"]
            item["rust_over_c"] = ratio
            summary.append(item)
            print(f"{name:6} {mode:5}  C {item['C']['median_us']:.3f} us, "
                  f"Rust {item['Rust']['median_us']:.3f} us, Rust/C {ratio:.3f}")
    (work / "results.json").write_text(json.dumps(summary, indent=2) + "\n")
    if any(item["rust_over_c"] > 1 for item in summary):
        raise SystemExit("PG19 parser performance requirement failed")


if __name__ == "__main__":
    main()
