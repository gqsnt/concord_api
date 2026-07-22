# Concepts and design

These pages explain the architectural choices behind Concord.

- [Design, code generation, and the managed lifecycle](design-and-lifecycle.md)
- [Security, bounded I/O, extension boundaries, and limitations](security-bounds-and-limitations.md)

The core idea is simple: the API declaration should look like the remote contract, while difficult cross-cutting behavior belongs to one private, finite, testable runtime.
