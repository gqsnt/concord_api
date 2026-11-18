// A client, a handle and a call are `Unpin` even when every input they hold is not,
// and so are the values a call returns: an input added later never takes `Unpin` away.
use concord::{by_offset, Json};
use std::marker::PhantomPinned;

#[derive(Clone)]
pub struct Sticky(PhantomPinned);

// The premise: `Sticky` is not `Unpin`. If it were, `Detect<_>` would be ambiguous.
const _: fn() = || {
    trait Detect<Marker> {
        fn test() {}
    }
    impl<T: ?Sized> Detect<()> for T {}
    struct IfUnpin;
    impl<T: ?Sized + Unpin> Detect<IfUnpin> for T {}
    let _ = <Sticky as Detect<_>>::test;
};

fn mark(_: &Sticky) -> u32 {
    1
}
fn maybe(sticky: &Option<Sticky>) -> Option<u32> {
    sticky.as_ref().map(mark)
}

concord::api! {
    pub client Sample(held: Sticky, spare?: Sticky) {
        base "https://example.invalid";
        read Json;
        query { "held" = mark(held); "spare" = maybe(spare); }
        scope group(member: Sticky, more?: Sticky) {
            query { "member" = mark(member); "more" = maybe(more); }
            GET fetch(needed: Sticky, extra?: Sticky) -> u32 {
                query { "needed" = mark(needed); "extra" = maybe(extra); }
            }
            GET list(start?: u32, extra?: Sticky) -> Vec<u32> {
                query { start; "extra" = maybe(extra); }
                paginate by_offset(start);
            }
        }
    }
}

fn unpin<T: Unpin>() {}
fn unpinned<T: Unpin>(_: &T) {}

fn main() {
    unpin::<Sample>();
    unpin::<sample::Group>();
    unpin::<sample::Call<sample::group::Fetch>>();
    unpin::<sample::Call<sample::group::List>>();

    let client = Sample::new(Sticky(PhantomPinned)).spare(Sticky(PhantomPinned));
    let group = client.group(Sticky(PhantomPinned)).more(Sticky(PhantomPinned));
    let fetch = group
        .fetch(Sticky(PhantomPinned))
        .extra(Sticky(PhantomPinned));
    unpinned(&fetch);
    unpinned(&fetch.clone().send());
    unpinned(&fetch.clone().send_full());
    unpinned(&fetch.clone().send_raw());
    unpinned(&fetch.send_if_none_match("\"v1\""));
    let list = group.list().extra(Sticky(PhantomPinned));
    unpinned(&list.clone().pages());
    unpinned(&list.pages().items());
}
