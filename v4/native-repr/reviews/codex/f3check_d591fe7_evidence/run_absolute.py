from pathlib import Path
import json, os, subprocess, sys
root=Path.cwd(); p=root/'target/probes'; lane=sys.argv[1]
env=os.environ.copy(); env['CARGO_TARGET_DIR']=str(root/f'target/probe-{lane}')
results=[]
for name in ['absolute_body','absolute_output','absolute_custom_body','absolute_alias_output']:
    cmd=['cargo']+(['+1.85'] if lane=='msrv' else [])+['check','--locked','--offline','--manifest-path',str(p/'renamed/Cargo.toml'),'--bin',name]
    with (p/f'{name}-{lane}.txt').open('w') as out:
        result=subprocess.run(cmd,env=env,stdout=out,stderr=subprocess.STDOUT)
    expected_failure=name in ['absolute_body','absolute_custom_body']
    results.append({'case':name,'command':cmd,'lane':lane,'exit_code':result.returncode,'expected_compile_failure':expected_failure})
    print(f'{lane} {name}: exit {result.returncode}',flush=True)
(p/f'absolute-results-{lane}.json').write_text(json.dumps(results,indent=2)+'\n')
