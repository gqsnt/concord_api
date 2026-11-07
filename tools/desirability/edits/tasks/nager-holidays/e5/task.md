# nager-holidays / e5: one provider document field (nearest variant: the response document)

**Provider changelog.** In every holiday object of the holiday array, the provider renamed the
boolean field `nationalHoliday` to `global` (same meaning, same values). The old name is no
longer sent. Nothing else changed.

Public names the hidden check uses:
- `Holiday::national_holiday` keeps its Rust name and meaning; it is decoded from `global`.
