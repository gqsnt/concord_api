## Slice-specific notes

This is the wave's simple control: keep the SDK as small as the slice allows. A timeout or
connection failure after a send has an unknown outcome (the email may have been accepted):
report it as such and never resend automatically.
