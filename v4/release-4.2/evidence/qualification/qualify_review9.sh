#!/usr/bin/env bash
set -uo pipefail
cd /home/canarit/projects/concord-v42/candidate
out=/home/canarit/projects/concord-v42/records/mutation; H=/home/canarit/projects/concord-v42/tools/heavy.sh
syn=crates/concord-macros/src/syntax.rs; ci=scripts/check-ci.py; cp $syn $out/syntax.rs.r9; cp $ci $out/check-ci.py.r9
run() { local name=$1 file=$2; shift 2
  python3 -I - "$file" "$1" "$2" <<'PY'
import sys; p,old,new=sys.argv[1:4]; s=open(p).read(); assert s.count(old)==1, old; open(p,'w').write(s.replace(old,new))
PY
  shift 2; echo "== $name: $*" > $out/$name.log
  timeout 1200 $H "$@" >> $out/$name.log 2>&1; echo "exit=$?" >> $out/$name.log; cp $out/syntax.rs.r9 $syn; cp $out/check-ci.py.r9 $ci
  cmp -s $syn $out/syntax.rs.r9 && cmp -s $ci $out/check-ci.py.r9 && echo restored-bytes-verified >> $out/$name.log
  echo "-- $name"; tail -2 $out/$name.log; grep -m3 -E "FAILED|FAIL:|panicked" $out/$name.log
}
run R19-written-delimiters $syn '    g.delimiter() == Delimiter::None
        && matches!' '    (true || g.delimiter() == Delimiter::None)
        && matches!' \
  cargo +1.99.1 test --locked -p concord-macros documented_initializers
run R20-control-prefixes $ci '                first = plain[0] if plain else other.words[0].text if other.words else other.ops[0]' '                first = other.words[0].text if other.words else other.ops[0]' \
  python3 -I -B -m unittest discover -s scripts/tests -p test_check_ci.py
git status --short
