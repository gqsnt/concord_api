import re, sys
p='Cargo.toml'
s=open(p).read()
m=re.search(r"<<<<<<< [^\n]*\n(.*?)=======\n(.*?)>>>>>>> [^\n]*\n", s, re.S)
names=[]
for part in (m.group(1), m.group(2)):
    for n in re.findall(r'"([^"]+)"', part):
        if n not in names: names.append(n)
anchors=("examples/github-sdk","examples/stripe-sdk","examples/shopify-sdk","examples/salesforce-sdk","examples/kubernetes-sdk")
cohort=sorted(n for n in names if n.startswith("examples/") and n.endswith("-sdk") and n not in anchors)
ordered=[n for n in names if n not in cohort]
i=ordered.index("examples/kubernetes-app")+1
ordered=ordered[:i]+cohort+ordered[i:]
s=s[:m.start()]+"members = ["+", ".join(f'"{n}"' for n in ordered)+"]\n"+s[m.end():]
open(p,'w').write(s)
print(cohort)
