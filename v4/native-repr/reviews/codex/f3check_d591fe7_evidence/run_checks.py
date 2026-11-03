from pathlib import Path
import json
import os
import subprocess
import sys
import time

root=Path.cwd(); base=root/'target/probes'
lane=sys.argv[1]
results=[]
for name in ['migration','matrix','renamed']:
    cmd=['cargo']+(['+1.85'] if lane=='msrv' else [])+['test','--locked','--offline','--manifest-path',str(base/name/'Cargo.toml'),'--lib','--','--nocapture','--test-threads=1']
    env=os.environ.copy()
    env['CARGO_TARGET_DIR']=str(root/f'target/probe-{lane}')
    out=base/f'{name}-{lane}.txt'
    print(f'RUN {lane} {name}',flush=True)
    start=time.monotonic()
    with out.open('w') as f:
        proc=subprocess.run(cmd,env=env,stdout=f,stderr=subprocess.STDOUT)
    results.append({'probe':name,'lane':lane,'command':cmd,'cargo_target_dir':env['CARGO_TARGET_DIR'],'exit_code':proc.returncode,'elapsed_seconds':time.monotonic()-start,'log':str(out)})
    (base/f'results-{lane}.json').write_text(json.dumps(results,indent=2)+'\n')
    print(f'DONE {lane} {name}: exit {proc.returncode}; {out}',flush=True)
