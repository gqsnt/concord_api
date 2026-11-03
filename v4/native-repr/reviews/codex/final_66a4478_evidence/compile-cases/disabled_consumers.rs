
wire::api! { pub client Api { base "http://127.0.0.1"; request json; response form; #[cfg(any())] scope gone { POST p(body p: Unknown) -> Unknown; } GET p(); } }
