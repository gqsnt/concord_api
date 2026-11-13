Observed limits under COMMON-WRITER rule 4; these are probes, not claimed contracts.

A52: mixed_site hides locals from authored expressions. It does not keep module
constants out of Rust pattern lookup. The constants.rs probe still gives E0530
and constant-pattern errors after every generated binding has mixed_site hygiene.
Changing locations with located_at does not alter this result. A plain macro_rules
probe has the same result (api/target/p44-const.rs).

Opus 1: a concrete non-Send/non-Sync problem is checked at multiple independent
Rust bounds in the fixed C1.11 layout. Opaque reader validation removes the extra
reader error, and manual Clone removes the extra input-Clone error. Rc<String>
still gives errors at __Types, page::Call and the engine's generic calls. The
macro has no access to Rust trait resolution, so it cannot stop emitting after
one failed Rust bound. The concrete_problem.rs probe records the remaining case.

The Slot table probe (reader-spans/12_slot.rs) also still gives three reports
for by_offset(42u32), at the authored tokens. They come from the bound and
opaque return type of the author-called by_offset helper. No second-error
suppression claim is made for this case either.
