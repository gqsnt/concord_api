"""Reconcile archived measurements only; never run a benchmark."""
import collections,json,pathlib,re,tarfile
root=pathlib.Path(__file__).resolve().parent
perf=root.parent.parent/'v4/native-repr/evidence/perf'
out=[]
def emit(s):out.append(s);print(s)
data=collections.defaultdict(dict)
with tarfile.open(perf/'criterion/raw-a915c8d-cand-aaa5a3c-run3.tar.xz') as t:
 members={m.name:m for m in t.getmembers() if m.isfile()}
 for name,m in members.items():
  if not name.endswith('/benchmark.json'):continue
  d=json.load(t.extractfile(m));stem=name[:-len('benchmark.json')]
  est=json.load(t.extractfile(members[stem+'estimates.json']))
  sample=json.load(t.extractfile(members[stem+'sample.json']))
  data[d['full_id']][name.split('/')[-2]]=(est['mean'],len(sample['times']))
emit('Archived Criterion IDs: '+str(dict(collections.Counter(k for v in data.values() for k in v))))
rows=[
 'construct/minimal/concord/prepare','construct/realistic_all_present/concord/prepare',
 'construct/realistic_optional_absent/concord/prepare','construct/s2_product_scoped/concord/prepare',
 'compose/inherit/concord/prepare','compose/deep8_reused_scope/concord/prepare',
 *[f'headers/{n}/concord_{kind}/prepare' for n in [16,4,1] for kind in ['text','numeric','native','literal']],
 'body_encode/bytes/100B/concord/prepare','body_encode/json/100B/concord/prepare',
 'body_encode/json/1MB/concord/prepare','decode/json/10KB/concord/decode','roundtrip/small/concord/send']
emit('ID | baseline / candidate run2 / run3 means in ns | max relative CI half-width | samples')
for id in rows:
 vals=data[id];means=[];widths=[];samples=[]
 for run in ['a915c8d','cand-aaa5a3c','run3-aaa5a3c']:
  if run not in vals:means.append('—');continue
  est,n=vals[run];v=est['point_estimate'];ci=est['confidence_interval'];means.append(f'{v:.3f}')
  widths.append(max(v-ci['lower_bound'],ci['upper_bound']-v)/v*100);samples.append(n)
 emit(f'{id} | '+ ' / '.join(means)+f' | {max(widths):.2f}% | {samples}')
emit('\nCompile rows: baseline / candidate seconds (sample count)')
ab=perf/'compile/ab-20261102T063238'
for p in sorted(ab.glob('*.suite-a915c8d.json')):
 q=p.with_name(p.name.replace('suite-a915c8d','suite-cand'))
 a=json.loads(p.read_text())['results'][0];b=json.loads(q.read_text())['results'][0]
 emit(f'{p.name.split(".suite-")[0]} | {a["mean"]:.6f} ± {a["stddev"]:.6f} / {b["mean"]:.6f} ± {b["stddev"]:.6f} ({len(a["times"])}, {len(b["times"])}) | ratio {b["mean"]/a["mean"]:.4f}')
emit('\nAllocations (prepare stage; no new measurements):')
allocs={}
for revision in ['a915c8d','aaa5a3c']:
 allocs[revision]={r['id']:r['stage_allocs'] for r in json.loads((perf/f'criterion/allocs-{revision}.json').read_text())}
for id in ['headers/16/concord_text','headers/16/concord_numeric','headers/16/concord_native','headers/16/concord_literal','headers/1/concord_native','headers/1/concord_literal','compose/inherit/concord','compose/deep8_reused_scope/concord','construct/realistic_all_present/concord']:
 emit(f'{id}: '+str([allocs[r].get(id) for r in allocs]))
for id,base in allocs['a915c8d'].items():
 if id.startswith('body_encode/') and id.endswith('/concord'):
  candidate=allocs['aaa5a3c'][id];assert candidate==base-1,(id,base,candidate)
emit('All nine Concord body encoding allocation rows: exactly one fewer than baseline.')
# Check every archived summary's displayed mean against raw estimates, including rows PERF omits.
for name,run in [('summary-a915c8d.md','a915c8d'),('summary-run2.md','cand-aaa5a3c'),('summary-run3.md','run3-aaa5a3c')]:
 checked=0
 for line in (perf/'criterion'/name).read_text().splitlines():
  m=re.match(r'\| `([^`]+)` \| ([0-9.]+) (ns|µs|ms|s) \|',line)
  if not m:continue
  id,number,unit=m.groups();v=data[id][run][0]['point_estimate'];scale={'ns':1,'µs':1e3,'ms':1e6,'s':1e9}[unit]
  tolerance=0.5*10**(-len(number.split('.')[-1]))*scale+1e-7
  assert abs(float(number)*scale-v)<=tolerance,(name,id,v,number)
  checked+=1
 emit(f'{name}: all {checked} displayed means agree with archived estimates to rounding.')
emit('\nRelease artifact size changes (PERF says all are within 0.5%):')
sizes={}
for line in (ab/'sizes.tsv').read_text().splitlines()[1:]:
 revision,artifact,size,_=line.split();sizes.setdefault(artifact,{})[revision]=int(size)
for artifact,values in sizes.items():
 a,b=values.values();emit(f'{artifact}: {a} -> {b} bytes ({(b/a-1)*100:+.4f}%)')
emit('\nPERF confidence interval exception omitted:')
e=data['construct/realistic_optional_absent/concord/prepare']['run3-aaa5a3c'][0]
emit(f'run3 optional absent: mean {e["point_estimate"]:.3f} ns; 95% CI {e["confidence_interval"]["lower_bound"]:.3f}..{e["confidence_interval"]["upper_bound"]:.3f} ns (not within +/-1%).')
(root/'perf-reconciliation.txt').write_text('\n'.join(out)+'\n')
