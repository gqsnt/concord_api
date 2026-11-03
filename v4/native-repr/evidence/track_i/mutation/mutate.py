"""Applies one named mutant to the worktree (python3 mutate.py NAME). Restore with git checkout."""
import sys
W = "/home/canarit/projects/concord_v4_native_work/wt/formats/"
def sub(path, old, new):
    p = W + path
    s = open(p).read()
    assert s.count(old) == 1, (path, old[:60])
    open(p, "w").write(s.replace(old, new))
name = sys.argv[1]
if name == "M1-drain-under-default":
    # An endpoint without `->` decodes with the nearest response default instead of draining.
    sub("crates/concord-macros/src/emit.rs",
        "        None => quote!(#k::__private::drain(&__META)),\n    };",
        """        None => match model.nearest_default(index, false) {
            Some(o) => {
                let f = format_type(k, &model.owners[o].response.as_ref().unwrap().format);
                quote!(#k::__private::decoder::<#f, ()>(&__META))
            }
            None => quote!(#k::__private::drain(&__META)),
        },
    };""")
elif name == "M2-ct-without-body":
    # An absent optional body is treated as an empty body with the format's media type.
    sub("crates/concord/src/__private.rs",
        "/// Encodes a body with its declared format",
        """/// MUTANT: an absent body as an empty one.
pub fn absent_body<T>(format: &'static BodyFormat<T>) -> Option<(reqwest::Body, Option<HeaderValue>)> {
    Some((reqwest::Body::from(Vec::<u8>::new()), format.content_type.clone()))
}

/// Encodes a body with its declared format""")
    sub("crates/concord-macros/src/emit.rs",
        "                        ::core::option::Option::None => ::core::option::Option::None,\n                    }",
        "                        ::core::option::Option::None => #k::__private::absent_body(&__FORMAT),\n                    }")
elif name == "M3-ct-sensitivity":
    # The inserted Content-Type ignores the key's static classification.
    sub("crates/concord/src/__private.rs",
        "                    if self.meta.sensitive_content_type {\n                        content_type.set_sensitive(true);\n                    }\n",
        "")
    sub("crates/concord/src/__private.rs",
        "            if let Some(mut content_type) = content_type {",
        "            if let Some(content_type) = content_type {")
elif name == "M4-8c-conflict":
    # No conflict check between a declared body media type and a surviving content-type header.
    sub("crates/concord-macros/src/model.rs",
        "    fn check_media_conflict(&self, endpoint: usize, composed: &Composed, errors: &mut Errors) {\n",
        "    fn check_media_conflict(&self, endpoint: usize, composed: &Composed, errors: &mut Errors) {\n        if true {\n            return;\n        }\n")
elif name == "M5-alias-cfg-gate":
    # The default's alias is emitted without the union of its consumers' cfgs.
    sub("crates/concord-macros/src/emit.rs",
        "            let cfg = consumers_cfg(model, d);\n",
        "            let cfg = { let _ = consumers_cfg(model, d); quote!() };\n")
else:
    sys.exit("unknown mutant " + name)
