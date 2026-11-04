#!/usr/bin/env bash
set -uo pipefail
cd /home/canarit/projects/concord-v42/candidate
out=/home/canarit/projects/concord-v42/records/mutation; H=/home/canarit/projects/concord-v42/tools/heavy.sh
syn=crates/concord-macros/src/syntax.rs
cp $syn $out/syntax.rs.r2
run() { local name=$1; shift
  python3 -I - "$syn" "$1" "$2" <<'PY'
import sys; p,old,new=sys.argv[1:4]; s=open(p).read(); assert s.count(old)==1, old; open(p,'w').write(s.replace(old,new))
PY
  shift 2; echo "== $name: $*" > $out/$name.log
  timeout 1200 $H "$@" >> $out/$name.log 2>&1; echo "exit=$?" >> $out/$name.log; cp $out/syntax.rs.r2 $syn
  cmp -s $syn $out/syntax.rs.r2 && echo restored-bytes-verified >> $out/$name.log
  echo "-- $name"; tail -2 $out/$name.log; grep -m3 -E "mismatch|FAILED|E0277|left:|right:|c >> d|pick!\(|Vec<u8>" $out/$name.log
}
run R4-verbatim '            let macro_input = bang;' '            let macro_input = bang && false;' \
  cargo +1.99.1 test --locked -p concord --test ui
run R5-number-dot '                        && (trailing_dot || integer && p.spacing() == Spacing::Alone) =>' '                        && false && (trailing_dot || integer && p.spacing() == Spacing::Alone) =>' \
  cargo +1.99.1 test --locked -p concord-macros
run R6-advice '    let t = forwarded(ty);' '    let t = false && forwarded(ty);' \
  cargo +1.99.1 test --locked -p concord --test ui
run R7-regroup '        out.extend([match tt {' '        out.extend([match tt { tt if true => tt,' \
  cargo +1.99.1 test --locked -p concord --test forwarded_types
git status --short crates/concord-macros
