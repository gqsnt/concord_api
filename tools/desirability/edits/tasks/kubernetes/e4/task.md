# kubernetes / e4: add a resource like its sibling, with its classification

**Provider changelog.** This SDK now needs StatefulSets (`apps/v1`, kind `StatefulSet`,
namespaced): `GET /apis/apps/v1/namespaces/{namespace}/statefulsets` (a chunked list exactly like
Deployments: the shared list parameters, `limit`/`continue`, the same `StatefulSetList`
envelope) and `GET /apis/apps/v1/namespaces/{namespace}/statefulsets/{name}`. Read at least
`metadata`, `spec.replicas` and `spec.serviceName`. Nothing else changed.

Add them next to the namespaced Deployment operations; the SDK's chunked-list adapter
(`list::ListPager`) must work for them like for the other lists.

Public names the hidden check uses:
- `ns.apps().stateful_sets()` (list, with the shared list setters) and
  `ns.apps().stateful_set(name: ObjectName)` (read);
- `model::StatefulSet` with public fields `metadata: ObjectMeta` and
  `spec: StatefulSetSpec { replicas: Option<i32>, service_name: Option<String> }`, and
  `model::StatefulSetList`;
- `<StatefulSet as Resource>::KIND == "StatefulSet"`, `API_VERSION == "apps/v1"`,
  `NAMESPACED == true`;
- `list::ListPager::new(ns.apps().stateful_sets(), limit)` pages it.
