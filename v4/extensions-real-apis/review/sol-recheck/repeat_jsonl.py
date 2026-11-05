from pathlib import Path
import subprocess, concurrent.futures, os, time
root=Path(__file__).resolve().parents[2]
bins=[p for p in (root/'review/sol-recheck/target/debug/deps').glob('jsonl_jobs-*') if p.is_file() and os.access(p,os.X_OK)]
assert len(bins)==1,bins
binary=bins[0]
def run(i):
    start=time.monotonic()
    result=subprocess.run([str(binary),'--exact','jsonl_jobs_hold_their_slot_through_a_stalled_body'],cwd=root,text=True,capture_output=True,timeout=20)
    assert result.returncode==0,(i,result.stdout,result.stderr)
    return time.monotonic()-start
with (root/'review/sol-recheck/logs/jsonl-repeat.log').open('w') as out:
    for workers,n in [(1,50),(8,100)]:
        with concurrent.futures.ThreadPoolExecutor(max_workers=workers) as pool:
            durations=list(pool.map(run,range(n)))
        print(f'PASS {n}/{n}; concurrent processes={workers}; min={min(durations):.3f}s max={max(durations):.3f}s mean={sum(durations)/n:.3f}s',file=out,flush=True)
