#!/usr/bin/env bash
set -uo pipefail
cd /home/canarit/projects/concord-v42/candidate
out=/home/canarit/projects/concord-v42/records/mutation; H=/home/canarit/projects/concord-v42/tools/heavy.sh
emit=crates/concord-macros/src/emit.rs; syn=crates/concord-macros/src/syntax.rs
cp $emit $out/emit.rs.r; cp $syn $out/syntax.rs.r
restore() { cp $out/emit.rs.r $emit; cp $out/syntax.rs.r $syn; }
run() { local name=$1 file=$2; shift 2
  python3 -I - "$file" "$1" "$2" <<'PY'
import sys; p,old,new=sys.argv[1:4]; s=open(p).read(); assert s.count(old)==1, old; open(p,'w').write(s.replace(old,new))
PY
  shift 2; echo "== $name: $*" > $out/$name.log
  timeout 1200 $H "$@" >> $out/$name.log 2>&1; echo "exit=$?" >> $out/$name.log; restore
  cmp -s $emit $out/emit.rs.r && cmp -s $syn $out/syntax.rs.r && echo restored-bytes-verified >> $out/$name.log
  echo "-- $name"; tail -2 $out/$name.log; grep -m4 -E "mismatch|FAILED|E0308|choose!\(\.\.\)|1 \+ 2 \* 2" $out/$name.log
}
run R1-join $syn 'if alone.is_some_and(|a| lexes_with(a, p.as_char()))' 'if false && alone.is_some_and(|a| lexes_with(a, p.as_char()))' \
  cargo +1.99.1 test --locked -p concord --test ui
run R2-invisible $syn 'if bare { ("", "") } else { ("(", ")") }' 'if bare || true { ("", "") } else { ("(", ")") }' \
  cargo +1.99.1 test --locked -p concord --test ui
run R3-located $emit 'if g.delimiter() == proc_macro2::Delimiter::None || invisible(g.stream()) =>' 'if g.delimiter() == proc_macro2::Delimiter::None =>' \
  cargo +1.99.1 test --locked -p concord --test forwarded_types
git status --short crates/concord-macros
