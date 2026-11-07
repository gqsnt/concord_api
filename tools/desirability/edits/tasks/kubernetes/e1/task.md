# kubernetes / e1: API group version

**Provider changelog.** The cluster now serves Ingress only under the `networking.k8s.io/v2`
group version: every Ingress path moved from `/apis/networking.k8s.io/v1/...` to
`/apis/networking.k8s.io/v2/...` (cluster-wide and namespaced lists, reads and watches), and
Ingress objects report `apiVersion: networking.k8s.io/v2`. Their fields are unchanged. Other
groups (`core/v1`, `apps/v1`, `batch/v1`) are unchanged. Nothing else changed.

Public names the hidden check uses (version-neutral names):
- `KubeContext::networking().ingresses()` and
  `KubeContext::namespace(ns).networking().ingresses()` / `.ingress(name)` request
  `/apis/networking.k8s.io/v2/...`;
- `<Ingress as Resource>::API_VERSION == "networking.k8s.io/v2"`.
(Renaming the generated `networking_v1` scope is allowed; the check does not use it.)
