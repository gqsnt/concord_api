/// A diff: `override` replaces a header that is set above.
GET diff() -> Bytes {
    headers { override "accept" = "application/vnd.github.diff"; }
    read Binary;
}
