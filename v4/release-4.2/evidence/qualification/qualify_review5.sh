#!/usr/bin/env bash
set -uo pipefail
cd /home/canarit/projects/concord-v42/candidate
out=/home/canarit/projects/concord-v42/records/mutation; H=/home/canarit/projects/concord-v42/tools/heavy.sh
syn=crates/concord-macros/src/syntax.rs; cp $syn $out/syntax.rs.r5
run() { local name=$1; shift
  python3 -I - "$syn" "$1" "$2" <<'PY'
import sys; p,old,new=sys.argv[1:4]; s=open(p).read(); assert s.count(old)==1, old; open(p,'w').write(s.replace(old,new))
PY
  shift 2; echo "== $name: $*" > $out/$name.log
  timeout 1200 $H "$@" >> $out/$name.log 2>&1; echo "exit=$?" >> $out/$name.log; cp $out/syntax.rs.r5 $syn
  cmp -s $syn $out/syntax.rs.r5 && echo restored-bytes-verified >> $out/$name.log
  echo "-- $name"; tail -2 $out/$name.log; grep -m3 -E "unnecessary parentheses|left:|right:|error\[E" $out/$name.log
}
run R10-call-site-span '                    (false, _) => rebuilt(&g, Delimiter::Parenthesis, Span::call_site(), true, false),' '                    (false, _) => rebuilt(&g, Delimiter::Parenthesis, g.span(), true, false),' \
  cargo +1.99.1 test --locked -p concord --test forwarded_types
run R11-expression-rule '                let bare = if expr {' '                let bare = if false && expr {' \
  cargo +1.99.1 test --locked -p concord --test forwarded_types
git status --short crates/concord-macros
