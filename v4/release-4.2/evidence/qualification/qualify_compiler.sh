#!/usr/bin/env bash
# Revert one compiler fix at a time in the candidate, run its maintained test, restore and verify bytes.
set -uo pipefail
cd /home/canarit/projects/concord-v42/candidate
out=/home/canarit/projects/concord-v42/records/mutation
H=/home/canarit/projects/concord-v42/tools/heavy.sh
emit=crates/concord-macros/src/emit.rs; syn=crates/concord-macros/src/syntax.rs
cp $emit $out/emit.rs.orig; cp $syn $out/syntax.rs.orig
sha_e=$(sha256sum $emit | cut -d' ' -f1); sha_s=$(sha256sum $syn | cut -d' ' -f1)
restore() { cp $out/emit.rs.orig $emit; cp $out/syntax.rs.orig $syn; }
run() { # name file pattern replacement cmd...
  local name=$1 file=$2; shift 2
  python3 -I - "$file" "$1" "$2" <<'PY'
import sys; p,old,new=sys.argv[1:4]; s=open(p).read(); assert s.count(old)==1, old; open(p,'w').write(s.replace(old,new))
PY
  shift 2
  echo "== $name: $*" > $out/$name.log
  timeout 1200 $H "$@" >> $out/$name.log 2>&1; echo "exit=$?" >> $out/$name.log
  restore
  [[ $(sha256sum $emit | cut -d' ' -f1) == $sha_e && $(sha256sum $syn | cut -d' ' -f1) == $sha_s ]] && echo "restored-bytes-verified" >> $out/$name.log
  tail -1 $out/$name.log; grep -m3 -E "error\[E0277\]|expected curly braces|panicked|mismatch|FAILED|Payload: Clone|a::b|body type" $out/$name.log
}
run F01-dedup $emit '        bounds.push(quote_spanned!(span=> for<'"'"'__concord> #t: ::core::clone::Clone));' \
'        if !bounds.iter().any(|b: &TokenStream| b.to_string() == quote_spanned!(span=> for<'"'"'__concord> #t: ::core::clone::Clone).to_string()) { bounds.push(quote_spanned!(span=> for<'"'"'__concord> #t: ::core::clone::Clone)); }' \
  cargo +1.99.1 test --locked -p concord-clone-hygiene-check --test clone_hygiene
run F02-guard $syn ' && !fork.peek(Token![!]) => {' ' => {' \
  cargo +1.99.1 test --locked -p concord --test output_types
run F03-replace $syn '    token_text(t.to_token_stream(), true)
}' '    t.to_token_stream().to_string().replace(" < ", "<").replace("< ", "<").replace(" >", ">").replace(" :: ", "::").replace(":: ", "::").replace(" ,", ",").replace("& ", "&")
}' \
  cargo +1.99.1 test --locked -p concord --test applied_advice --test ui
run F04-plain $syn '                            name,
                            if optional { "?" } else { "" },' '                            name.to_string().trim_start_matches("r#"),
                            if optional { "?" } else { "" },' \
  cargo +1.99.1 test --locked -p concord --test applied_advice --test ui
git status --short crates/concord-macros
