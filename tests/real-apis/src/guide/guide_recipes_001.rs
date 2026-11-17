concord::api! {
pub client Docker(origin: concord::Base) {
        base origin;
        GET logs(container: String) -> concord::ByteStream {
            path "containers/{container}/logs";
            query { "stdout" = true; "stderr" = true; "follow" = true; }
            read concord::read::Stream;
        }
    }
}
