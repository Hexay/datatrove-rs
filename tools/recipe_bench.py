"""End-to-end pipelines through datatrove's LocalPipelineExecutor (1 task), datatrove vs datatrove_rs.

Usage:
  recipe_bench.py <out.json> [reps=3] [scenario...]   alternate datatrove/datatrove_rs runs, write all of them
  recipe_bench.py --run <scenario> <pure|rs> <work_dir>  one measured run in this process, JSON on stdout

Scenarios (paths from env):
  fineweb   FineWeb recipe on the first 5,000 records of $RECIPE_WARC: WarcReader, URLFilter, Trafilatura,
            LanguageFilter(en), the four quality filters, JsonlWriter
  fineweb2  $RECIPE_FW2_PARQUET (one FineWeb-2 language, $RECIPE_FW2_LANG): the four quality filters,
            JsonlWriter, MinhashDedupSignature

Each run is a fresh process taking one light.lock slot (tools/slot.sh). `rs` calls `datatrove_rs.patch()`, which
is what DATATROVE_RS=1 does. Wall and CPU (process + reaped children, i.e. the Trafilatura sandbox) cover
`executor.run()` only; the 1-minute load average is sampled before and after, and `throttled` says whether the
cgroup hit memory.high during the run. Outputs go to $RECIPE_WORK (keep it off tmpfs: tmpfs counts as memory).
tools/make_chart.py draws the result.
"""

import glob
import gzip
import hashlib
import json
import os
import resource
import shutil
import statistics
import subprocess
import sys
import time

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
CACHE = "/srv/agents/cache"
WARC = os.environ.get("RECIPE_WARC", f"{CACHE}/fineweb-profile/warc/CC-MAIN-20240220211055-20240221001055-00000.warc.gz")
FW2 = os.environ.get("RECIPE_FW2_PARQUET", f"{CACHE}/fineweb2/de.parquet")
FW2_LANG = os.environ.get("RECIPE_FW2_LANG", "deu_Latn")
N_RECORDS = 5000


def fineweb(work):
    from datatrove.pipeline.extractors import Trafilatura
    from datatrove.pipeline.filters import (C4QualityFilter, FineWebQualityFilter, GopherQualityFilter,
                                            GopherRepetitionFilter, LanguageFilter, URLFilter)
    from datatrove.pipeline.readers import WarcReader
    from datatrove.pipeline.writers.jsonl import JsonlWriter

    folder, name = os.path.split(WARC)
    return [
        WarcReader(folder, glob_pattern=name, limit=N_RECORDS, default_metadata={"dump": "CC-MAIN-2024-10"}),
        URLFilter(),
        Trafilatura(favour_precision=True),
        LanguageFilter(languages=["en"]),
        GopherRepetitionFilter(),
        GopherQualityFilter(),
        C4QualityFilter(),
        FineWebQualityFilter(),
        JsonlWriter(f"{work}/output"),
    ]


def fineweb2(work):
    from datatrove.pipeline.dedup import MinhashDedupSignature
    from datatrove.pipeline.filters import (C4QualityFilter, FineWebQualityFilter, GopherQualityFilter,
                                            GopherRepetitionFilter)
    from datatrove.pipeline.readers import ParquetReader
    from datatrove.pipeline.writers.jsonl import JsonlWriter

    folder, name = os.path.split(FW2)
    lang = {"language": FW2_LANG}
    return [
        ParquetReader(folder, glob_pattern=name),
        GopherRepetitionFilter(**lang),
        GopherQualityFilter(**lang),
        C4QualityFilter(**lang),
        FineWebQualityFilter(**lang),
        JsonlWriter(f"{work}/output"),
        MinhashDedupSignature(f"{work}/output/signatures", **lang),
    ]


SCENARIOS = {"fineweb": fineweb, "fineweb2": fineweb2}


def digest(work):
    """sha256 over every output file, gunzipped, in path order: equal digests mean identical output."""
    h = hashlib.sha256()
    for path in sorted(glob.glob(f"{work}/output/**/*", recursive=True)):
        if os.path.isfile(path):
            h.update(path[len(work):].encode())
            with (gzip.open if path.endswith(".gz") else open)(path, "rb") as f:
                h.update(f.read())
    return h.hexdigest()


def cpu_now():
    c = resource.getrusage(resource.RUSAGE_CHILDREN)
    return time.process_time() + c.ru_utime + c.ru_stime


def cgroup_read(name):
    """A file of this process's cgroup v2 directory, or None off cgroup v2."""
    try:
        with open("/proc/self/cgroup") as f:
            path = f.read().strip().split(":", 2)[2]
        with open(f"/sys/fs/cgroup{path}/{name}") as f:
            return f.read()
    except (OSError, IndexError):
        return None


def throttle_events():
    """memory.high event count: a run that raises it was memory-throttled and its times are void."""
    events = cgroup_read("memory.events")
    return None if events is None else int(dict(line.split() for line in events.splitlines())["high"])


def wait_for_headroom(limit_s=3600):
    """On a shared box, wait until the cgroup has $RECIPE_HEADROOM_MB (default 1280; a run peaks below 900 MB RSS)
    below memory.high, or give up after limit_s."""
    need = int(os.environ.get("RECIPE_HEADROOM_MB", "1280")) << 20
    for _ in range(limit_s // 10):
        cur, high = cgroup_read("memory.current"), cgroup_read("memory.high")
        if cur is None or high is None or high.strip() == "max" or int(cur) + need < int(high):
            return
        time.sleep(10)


def versions():
    from importlib.metadata import PackageNotFoundError, version

    out = {"python": ".".join(map(str, sys.version_info[:3]))}
    for dist in ("datatrove", "datatrove_rs", "trafilatura_rs", "spacy"):
        try:
            out[dist] = version(dist.replace("_", "-"))
        except PackageNotFoundError:
            out[dist] = None
    return out


def run_one(scenario, variant, work):
    if variant == "rs":
        import datatrove_rs

        datatrove_rs.patch()
    from datatrove.executor import LocalPipelineExecutor
    from loguru import logger

    logger.remove()
    shutil.rmtree(work, ignore_errors=True)
    ex = LocalPipelineExecutor(pipeline=SCENARIOS[scenario](work), tasks=1, workers=1, logging_dir=f"{work}/logs")
    load0, thr0, c0, w0 = os.getloadavg()[0], throttle_events(), cpu_now(), time.perf_counter()
    stats = ex.run()
    wall, cpu, load1, thr1 = time.perf_counter() - w0, cpu_now() - c0, os.getloadavg()[0], throttle_events()
    rss = max(resource.getrusage(r).ru_maxrss for r in (resource.RUSAGE_SELF, resource.RUSAGE_CHILDREN)) / 1024
    steps = [{"name": s.name, "time": s.time_stats.total, "stats": {k: getattr(v, "total", v) for k, v in s.stats.items()}}
             for s in stats.stats]
    return {"wall": wall, "cpu": cpu, "load": [load0, load1], "peak_rss_mb": rss,
            "throttled": None if thr0 is None else thr1 > thr0, "digest": digest(work), "steps": steps}


def measured(scenario, variant, work, tries=4):
    """One unthrottled run in a fresh process holding a light.lock slot; throttled runs are retried."""
    cmd = f'. "{ROOT}/tools/slot.sh" && in_light_slot "$@"'
    for _ in range(tries):
        wait_for_headroom()
        out = subprocess.run(["bash", "-c", cmd, "slot", sys.executable, __file__, "--run", scenario, variant, work],
                             check=True, stdout=subprocess.PIPE, text=True).stdout
        r = json.loads(out.strip().splitlines()[-1])
        if not r["throttled"]:
            return r
        print(f"{scenario} {variant}: memory-throttled, retrying", flush=True)
    sys.exit(f"{scenario} {variant}: throttled {tries} times")


def main(out_path, reps=3, scenarios=tuple(SCENARIOS)):
    work = os.environ.get("RECIPE_WORK", os.path.join(os.environ.get("TMPDIR", "/tmp"), "datatrove_rs_recipe"))
    results = {"date": time.strftime("%Y-%m-%d"), "cpus": os.cpu_count(), "reps": reps, "versions": versions(),
               "scenarios": {}}
    for scenario in scenarios:
        runs = {"pure": [], "rs": []}
        for rep in range(reps):
            for variant in ("pure", "rs") if rep % 2 == 0 else ("rs", "pure"):
                r = measured(scenario, variant, f"{work}/{scenario}_{variant}")
                runs[variant].append(r)
                print(f"{scenario} {variant} rep{rep}: wall {r['wall']:.1f} s, cpu {r['cpu']:.1f} s, "
                      f"load {r['load'][0]:.1f}-{r['load'][1]:.1f}, rss {r['peak_rss_mb']:.0f} MB, "
                      f"throttled {r['throttled']}, {r['digest'][:12]}", flush=True)
        digests = {r["digest"] for v in runs.values() for r in v}
        results["scenarios"][scenario] = {
            "runs": runs, "identical_output": len(digests) == 1,
            **{f"median_{k}": {v: statistics.median(r[k] for r in runs[v]) for v in runs} for k in ("wall", "cpu")},
        }
        print(scenario, {k: results["scenarios"][scenario][k] for k in ("median_wall", "median_cpu", "identical_output")})
    with open(out_path, "w") as f:
        json.dump(results, f, indent=1)


if __name__ == "__main__":
    if sys.argv[1] == "--run":
        print(json.dumps(run_one(*sys.argv[2:5])))
    else:
        main(sys.argv[1], int(sys.argv[2]) if len(sys.argv) > 2 else 3, tuple(sys.argv[3:]) or tuple(SCENARIOS))
