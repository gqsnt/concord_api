from pathlib import Path
import subprocess,json,os,sys
root=Path.cwd(); crate=root/'target/probes/focused'; results=[]
for tool in ['stable','1.85']:
 for name,c in json.loads((crate/'compile-cases.json').read_text()).items():
  cmd=['cargo']+(['+1.85'] if tool=='1.85' else [])+['check','--locked','--offline','--manifest-path',str(crate/'Cargo.toml'),'--bin',name]
  p=subprocess.run(cmd,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,env={**os.environ,'CARGO_TARGET_DIR':str(root/'target/probe-build')})
  (crate/f'{name}.{tool}.txt').write_text(p.stdout)
  passed=(p.returncode==0)==c['ok'] and (c['ok'] or c['diagnostic'] in p.stdout)
  results.append({'name':name,'toolchain':tool,'expected_compile':c['ok'],'exit':p.returncode,'passed':passed})
  print(tool,name,'PASS' if passed else 'FAIL',flush=True)
(root/'target/probes/compile-results.json').write_text(json.dumps(results,indent=2)+'\n')
sys.exit(not all(r['passed'] for r in results))
