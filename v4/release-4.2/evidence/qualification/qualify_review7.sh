#!/usr/bin/env bash
set -uo pipefail
cd /home/canarit/projects/concord-v42/candidate
out=/home/canarit/projects/concord-v42/records/mutation; H=/home/canarit/projects/concord-v42/tools/heavy.sh
syn=crates/concord-macros/src/syntax.rs; ci=scripts/check-ci.py; cp $syn $out/syntax.rs.r7; cp $ci $out/check-ci.py.r7
run() { local name=$1 file=$2; shift 2
  python3 -I - "$file" "$1" "$2" <<'PY'
import sys; p,old,new=sys.argv[1:4]; s=open(p).read(); assert s.count(old)==1, old; open(p,'w').write(s.replace(old,new))
PY
  shift 2; echo "== $name: $*" > $out/$name.log
  timeout 1200 $H "$@" >> $out/$name.log 2>&1; echo "exit=$?" >> $out/$name.log; cp $out/syntax.rs.r7 $syn; cp $out/check-ci.py.r7 $ci
  cmp -s $syn $out/syntax.rs.r7 && cmp -s $ci $out/check-ci.py.r7 && echo restored-bytes-verified >> $out/$name.log
  echo "-- $name"; tail -2 $out/$name.log; grep -m3 -E "ambiguous|range pattern bounds|FAIL:|error\[E" $out/$name.log
}
run R15-type-with-plus $syn '            || syn::parse::Parser::parse2(Type::parse, stream.clone()).is_ok()' '            || false && syn::parse::Parser::parse2(Type::parse, stream.clone()).is_ok()' \
  cargo +1.99.1 test --locked -p concord --test forwarded_types
run R16-literal $syn '        && !literal' '        && (true || !literal)' \
  cargo +1.99.1 test --locked -p concord --test forwarded_types
run R17-prefixes $ci '    while words and words[0] in ("builtin", "command"):' '    while False and words and words[0] in ("builtin", "command"):' \
  python3 -I -B -m unittest discover -s scripts/tests -p test_check_ci.py
git status --short
