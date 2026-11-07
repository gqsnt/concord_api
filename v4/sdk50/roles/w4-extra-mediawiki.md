## Slice-specific notes

This is the wave's simple control: keep the SDK as small as the slice allows. The caller sets an
identifying User-Agent. A query result's `continue` object may hold several fields, and every one
of them must be sent back on the next request; dropping one can repeat pages or silently truncate
the listing.
