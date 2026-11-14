//! Compiled regression clients and request assertions.
#![allow(dead_code, unused_imports)]
#[doc(hidden)]
pub mod support;
#[doc(hidden)]
mod description_009 {
    use super::support::*;
    include!("description_009.rs");
}
#[doc(hidden)]
mod description_010 {
    use super::support::*;
    include!("description_010.rs");
}
#[doc(hidden)]
mod description_011 {
    use super::support::*;
    include!("description_011.rs");
}
#[doc(hidden)]
mod description_012 {
    use super::support::*;
    include!("description_012.rs");
}
#[doc(hidden)]
mod description_013 {
    use super::support::*;
    include!("description_013.rs");
}
#[doc(hidden)]
mod description_014 {
    use super::support::*;
    include!("description_014.rs");
}
#[doc(hidden)]
mod description_015 {
    use super::support::*;
    include!("description_015.rs");
}
#[doc(hidden)]
mod description_016 {
    use super::support::*;
    include!("description_016.rs");
}
#[doc(hidden)]
mod description_017 {
    use super::support::*;
    include!("description_017.rs");
}
#[doc(hidden)]
mod description_018 {
    use super::support::*;
    include!("description_018.rs");
}
#[doc(hidden)]
mod description_019 {
    use super::support::*;
    include!("description_019.rs");
}
#[doc(hidden)]
mod description_020 {
    use super::support::*;
    include!("description_020.rs");
}
#[doc(hidden)]
mod description_022 {
    use super::support::*;
    include!("description_022.rs");
}
#[doc(hidden)]
mod description_023 {
    use super::support::*;
    include!("description_023.rs");
}
#[doc(hidden)]
mod description_024 {
    use super::support::*;
    include!("description_024.rs");
}
#[doc(hidden)]
mod description_025 {
    use super::support::*;
    include!("description_025.rs");
}
#[doc(hidden)]
mod description_027 {
    use super::support::*;
    include!("description_027.rs");
}
#[doc(hidden)]
mod description_029 {
    use super::support::*;
    include!("description_029.rs");
}
#[doc(hidden)]
mod description_030 {
    use super::support::*;
    include!("description_030.rs");
}
#[doc(hidden)]
mod description_031 {
    use super::support::*;
    include!("description_031.rs");
}
#[doc(hidden)]
mod description_032 {
    use super::support::*;
    include!("description_032.rs");
}
#[doc(hidden)]
mod description_001 {
    use super::support::*;
    include!("description_001.rs");
}
#[doc(hidden)]
mod description_002 {
    use super::support::*;
    include!("description_002.rs");
}
#[doc(hidden)]
mod description_003 {
    use super::support::*;
    include!("description_003.rs");
}
#[doc(hidden)]
mod description_004 {
    use super::support::*;
    include!("description_004.rs");
}
#[doc(hidden)]
mod description_005 {
    use super::support::*;
    include!("description_005.rs");
}
#[doc(hidden)]
mod description_006 {
    use super::support::*;
    include!("description_006.rs");
}
#[doc(hidden)]
mod description_007 {
    use super::support::*;
    include!("description_007.rs");
}
#[doc(hidden)]
mod description_008 {
    use super::support::*;
    include!("description_008.rs");
}
#[doc(hidden)]
mod description_033 {
    use super::support::*;
    include!("description_033.rs");
}
#[doc(hidden)]
mod description_034 {
    use super::support::*;
    include!("description_034.rs");
}
#[doc(hidden)]
mod description_035 {
    use super::support::*;
    include!("description_035.rs");
}
#[doc(hidden)]
mod description_036 {
    use super::support::*;
    include!("description_036.rs");
}
#[doc(hidden)]
mod description_038 {
    use super::support::*;
    include!("description_038.rs");
}
#[doc(hidden)]
mod description_040 {
    use super::support::*;
    include!("description_040.rs");
}
#[doc(hidden)]
mod description_041 {
    use super::support::*;
    include!("description_041.rs");
}
#[doc(hidden)]
mod description_042 {
    use super::support::*;
    include!("description_042.rs");
}
#[doc(hidden)]
mod description_043 {
    use super::support::*;
    include!("description_043.rs");
}
#[doc(hidden)]
mod description_044 {
    use super::support::*;
    include!("description_044.rs");
}
#[doc(hidden)]
mod description_045 {
    use super::support::*;
    include!("description_045.rs");
}

#[doc(hidden)]
mod description_021 {
    use super::support::*;
    include!("description_021.rs");
}

#[doc(hidden)]
mod description_026 {
    use super::support::*;
    include!("description_026.rs");
}

#[doc(hidden)]
mod description_028 {
    use super::support::*;
    include!("description_028.rs");
}

#[doc(hidden)]
mod description_037 {
    use super::support::*;
    include!("description_037.rs");
}

#[doc(hidden)]
mod description_039 {
    use super::support::*;
    include!("description_039.rs");
}

#[cfg(test)]
mod requests {
    use super::*;
    #[test]
    fn c1_20_regression_clients_build_requests() {
        use support::*;
        let stripe = description_037::Stripe::new(Secret::new("fixture"));
        assert_eq!(
            stripe.customers().list().request().unwrap().url().path(),
            "/v1/customers"
        );
        assert_eq!(
            stripe.invoices().list().request().unwrap().url().path(),
            "/v1/invoices"
        );
        description_039::Fixture::new()
            .create(NewDoc("doc".into()))
            .request()
            .unwrap();
        description_026::Fixture::new()
            .get(DocId("id".into()))
            .request()
            .unwrap();
        description_028::Fixture::new()
            .create(NewTask("task".into()))
            .request()
            .unwrap();
    }
}
