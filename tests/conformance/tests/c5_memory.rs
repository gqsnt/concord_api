//! C5.4: this test bounds how much memory a reader reserves at once.
//!
//! The test reads the peak of the virtual memory of the process (`VmPeak`, Linux): a
//! reservation shows there even when it is freed at once and never touched. It is alone in
//! its binary, so that no other test moves that peak while it runs.

mod common;

#[cfg(target_os = "linux")]
mod linux {
    use crate::common::engine::*;
    use bytes::Bytes;
    use concord::error::{Class, Kind};
    use concord::read::{Incoming, ReadError, Reader};
    use concord_testkit::Script;

    const GIB: usize = 1 << 30;

    /// A reader of the caller's own, with a limit of 1 GiB.
    #[derive(Clone, Copy)]
    struct UpToOneGib;

    impl Reader<Bytes> for UpToOneGib {
        async fn read(&self, incoming: Incoming) -> Result<Bytes, ReadError> {
            incoming.bytes(GIB).await
        }
    }

    /// The peak of the virtual memory of this process, in KiB.
    fn peak() -> u64 {
        let status = std::fs::read_to_string("/proc/self/status").expect("the status");
        status
            .lines()
            .find_map(|line| line.strip_prefix("VmPeak:"))
            .and_then(|rest| rest.trim().trim_end_matches("kB").trim().parse().ok())
            .expect("a VmPeak line")
    }

    #[tokio::test(start_paused = true)]
    async fn c5_4_the_announced_size_never_decides_what_is_reserved_at_once() {
        // "the engine never reserves more than what has arrived, doubled,
        // with a floor of 64 KiB, however large the limit." The head announces 1 GiB, within
        // the limit; two bytes come, in two frames, then the body ends.
        let script = Script::new();
        script
            .answer(200)
            .declare_length(GIB as u64)
            .body("a")
            .body("b");
        let core = concord::__private::Core::default().with_transport(script);
        let call = prepared_get(core, UpToOneGib);

        let before = peak();
        let error = failure(call.send().await);
        let grown = peak().saturating_sub(before);

        // a body that ends before its announced size is a Transport error of class Body.
        assert_eq!(error.kind(), Kind::Transport);
        assert_eq!(error.class(), Some(Class::Body));
        // Two bytes arrived: at most 64 KiB may be reserved. The bound checked is 256 MiB,
        // far over that and far under the 1 GiB that was announced.
        assert!(
            grown < 256 * 1024,
            "the peak of virtual memory grew by {grown} KiB while 2 bytes arrived"
        );
    }
}
