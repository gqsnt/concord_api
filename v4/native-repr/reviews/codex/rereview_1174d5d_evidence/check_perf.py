from pathlib import Path
import tarfile,json,collections,re
root=Path('v4/native-repr/evidence/perf'); rows={}; counts=collections.Counter()
with tarfile.open(root/'criterion/raw-a915c8d-cand-aaa5a3c-run3.tar.xz') as tf:
 for m in tf.getmembers():
  if not m.name.endswith('/estimates.json'):continue
  folder=m.name.rsplit('/',1)[0]; run=folder.rsplit('/',1)[-1]
  if run not in ['a915c8d','cand-aaa5a3c','run3-aaa5a3c']:continue
  b=json.load(tf.extractfile(folder+'/benchmark.json')); e=json.load(tf.extractfile(m))['mean']
  rows[(b['full_id'],run)]=e; counts[run]+=1
print('Archive estimate counts:',dict(counts))
ids=['construct/minimal/concord/prepare','construct/realistic_all_present/concord/prepare','construct/realistic_optional_absent/concord/prepare','construct/s2_product_scoped/concord/prepare','compose/inherit/concord/prepare','compose/deep8_reused_scope/concord/prepare','headers/16/concord_text/prepare','headers/16/concord_numeric/prepare','headers/16/concord_native/prepare','headers/16/concord_literal/prepare','headers/4/concord_native/prepare','headers/4/concord_literal/prepare','headers/1/concord_native/prepare','headers/1/concord_literal/prepare','body_encode/bytes/100B/concord/prepare','body_encode/json/100B/concord/prepare','body_encode/json/1MB/concord/prepare','decode/json/10KB/concord/decode','roundtrip/small/concord/send']
exceptions=[]
for id in ids:
 for run in ['a915c8d','cand-aaa5a3c','run3-aaa5a3c']:
  if (id,run) not in rows: continue
  e=rows[id,run]; mean=e['point_estimate']; lo=e['confidence_interval']['lower_bound'];hi=e['confidence_interval']['upper_bound']; width=max(mean-lo,hi-mean)/mean*100
  print(f'{id} | {run} | mean {mean:.6f} ns | 95% CI {lo:.6f}..{hi:.6f} | max deviation {width:.4f}%')
  if width>1:exceptions.append((id,run,width))
assert len(exceptions)==2,exceptions
assert round(exceptions[0 if 'optional' in exceptions[0][0] else 1][2],1)==8.6
print('Exactly the two documented confidence exceptions:',exceptions)
print('Release sizes:')
sizes={}
for line in (root/'compile/ab-20261102T063238/sizes.tsv').read_text().splitlines()[1:]:
 suite,name,size,_=line.split('\t');sizes[suite,name]=int(size)
changes=[]
for suite,name in sizes:
 if suite!='suite-a915c8d':continue
 a=sizes[suite,name];b=sizes['suite-cand',name];pct=(b/a-1)*100;changes.append((name,pct));print(f'{name}: {a} -> {b} bytes ({pct:+.6f}%)')
rlibs=[p for n,p in changes if n.endswith('.rlib')];bins=[p for n,p in changes if not n.endswith('.rlib')]
assert round(min(rlibs),2)==0.03 and round(max(rlibs),2)==1.15
assert max(abs(p) for p in bins)<0.12
perf=Path('v4/native-repr/PERF.md').read_text();assert '±8.6%' in perf and '±43%' in perf and '+0.03% to +1.15%' in perf
print('PASS: corrected PERF.md F4 statements match archived estimates and sizes. No benchmark executed.')
