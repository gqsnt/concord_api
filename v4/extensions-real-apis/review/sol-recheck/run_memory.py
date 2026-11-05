from pathlib import Path
import os, subprocess, re
root=Path(__file__).resolve().parents[2]
env=dict(os.environ, CARGO_TARGET_DIR=str(root/'review/sol-recheck/target'), TMPDIR=str(root/'review/sol-recheck/tmp'))
manifest='review/sol-recheck/probes/Cargo.toml'
routes=['record, finite','record, lazy','payload columns, finite','payload columns, lazy','inferred names, finite','inferred names, lazy','configured columns, finite','configured columns, lazy']
log=root/'review/sol-recheck/logs/independent-memory.log'
with log.open('w') as out:
    for baseline in [True,False]:
        cmd=['cargo','build','--offline','--manifest-path',manifest,'--bin','header_memory']
        if baseline: cmd+=['--features','before']
        subprocess.run(cmd,cwd=root,env=env,stdout=out,stderr=out,check=True)
        for mib in [16,32]:
            for route in routes:
                result=subprocess.run([str(root/'review/sol-recheck/target/debug/header_memory'),route,str(mib)],cwd=root,env=env,text=True,capture_output=True,check=True)
                print(('BEFORE' if baseline else 'CURRENT')+' '+result.stdout.strip(),file=out,flush=True)
                kib=int(re.search(r'growth=(\d+) KiB',result.stdout).group(1))
                if not baseline: assert kib<4096,(route,mib,kib)
                elif 'inferred' in route: assert kib>mib*1024*1.5,(route,mib,kib)
                elif 'configured' in route: assert kib>mib*1024*0.7,(route,mib,kib)
    print('PASS: separate processes, 8 routes x 2 sizes x 2 implementations; current <4 MiB each; old inferred/configured positive controls detected',file=out)
