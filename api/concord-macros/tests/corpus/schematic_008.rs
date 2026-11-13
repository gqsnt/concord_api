PUT put(…, metadata: BTreeMap<String, String>) { headers { "x-amz-meta-*" = metadata; } }
