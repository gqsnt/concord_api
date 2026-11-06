import re, subprocess, sys
branch=sys.argv[1]
ours=open('Cargo.lock').read()
theirs=subprocess.run(['git','show',f'{branch}:Cargo.lock'],capture_output=True,text=True,check=True).stdout
def pkgs(t):
    out={}
    for b in t.split('\n[[package]]\n')[1:]:
        n=re.search(r'name = "([^"]+)"',b).group(1); v=re.search(r'version = "([^"]+)"',b).group(1)
        out[(n,v)]=b.rstrip('\n')
    return out
po=pkgs(ours); pt=pkgs(theirs)
new=[k for k in pt if k not in po]
head=ours.split('\n[[package]]\n')[0]
allp=dict(po); allp.update({k:pt[k] for k in new})
keys=sorted(allp)
open('Cargo.lock','w').write(head+''.join('\n[[package]]\n'+allp[k]+'\n' for k in keys))
print(len(new),'new packages from',branch, [k[0] for k in new][:8])
