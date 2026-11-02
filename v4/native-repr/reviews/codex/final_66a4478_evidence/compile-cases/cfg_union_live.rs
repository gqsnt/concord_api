
wire::api! { pub client Api { base "http://127.0.0.1"; request json; #[cfg(any())] POST p(body p: Unknown); #[cfg(all())] POST q(body p: String); } }
