//! Every guide root, compiled with inert surrounding declarations.
#![allow(dead_code, unused_imports)]
#[doc(hidden)]
pub mod support;
#[doc(hidden)]
mod root_3_language_001 {
    use super::support::*;
    include!("3-language_001.rs");
}
#[doc(hidden)]
mod root_3_language_002 {
    use super::support::*;
    include!("3-language_002.rs");
}
#[doc(hidden)]
mod root_3_language_003 {
    use super::support::*;
    include!("3-language_003.rs");
}
#[doc(hidden)]
mod root_3_language_004 {
    use super::support::*;
    include!("3-language_004.rs");
}
#[doc(hidden)]
mod root_3_language_005 {
    use super::support::*;
    include!("3-language_005.rs");
}
#[doc(hidden)]
mod root_3_language_006 {
    use super::support::*;
    include!("3-language_006.rs");
}
#[doc(hidden)]
mod root_3_language_007 {
    use super::support::*;
    include!("3-language_007.rs");
}
#[doc(hidden)]
mod root_3_language_008 {
    use super::support::*;
    include!("3-language_008.rs");
}
#[doc(hidden)]
mod root_3_language_009 {
    use super::support::*;
    include!("3-language_009.rs");
}
#[doc(hidden)]
mod root_3_language_010 {
    use super::support::*;
    include!("3-language_010.rs");
}
#[doc(hidden)]
mod root_3_language_011 {
    use super::support::*;
    include!("3-language_011.rs");
}
#[doc(hidden)]
mod root_3_language_012 {
    use super::support::*;
    include!("3-language_012.rs");
}
#[doc(hidden)]
mod root_5_response_001 {
    use super::support::*;
    include!("5-response_001.rs");
}
#[doc(hidden)]
mod root_5_response_002 {
    use super::support::*;
    include!("5-response_002.rs");
}
#[doc(hidden)]
mod root_5_response_003 {
    use super::support::*;
    include!("5-response_003.rs");
}
#[doc(hidden)]
mod root_5_response_004 {
    use super::support::*;
    include!("5-response_004.rs");
}
#[doc(hidden)]
mod root_5_response_006 {
    use super::support::*;
    include!("5-response_006.rs");
}
#[doc(hidden)]
mod root_5_response_008 {
    use super::support::*;
    include!("5-response_008.rs");
}
#[doc(hidden)]
mod root_5_response_009 {
    use super::support::*;
    include!("5-response_009.rs");
}
#[doc(hidden)]
mod root_5_response_010 {
    use super::support::*;
    include!("5-response_010.rs");
}
#[doc(hidden)]
mod root_5_response_011 {
    use super::support::*;
    include!("5-response_011.rs");
}
#[doc(hidden)]
mod root_6_behaviour_001 {
    use super::support::*;
    include!("6-behaviour_001.rs");
}
#[doc(hidden)]
mod root_6_behaviour_002 {
    use super::support::*;
    include!("6-behaviour_002.rs");
}
#[doc(hidden)]
mod root_6_behaviour_003 {
    use super::support::*;
    include!("6-behaviour_003.rs");
}
#[doc(hidden)]
mod root_6_behaviour_004 {
    use super::support::*;
    include!("6-behaviour_004.rs");
}
#[doc(hidden)]
mod root_6_behaviour_005 {
    use super::support::*;
    include!("6-behaviour_005.rs");
}
#[doc(hidden)]
mod root_6_behaviour_006 {
    use super::support::*;
    include!("6-behaviour_006.rs");
}
#[doc(hidden)]
mod root_6_behaviour_007 {
    use super::support::*;
    include!("6-behaviour_007.rs");
}
#[doc(hidden)]
mod root_6_behaviour_008 {
    use super::support::*;
    include!("6-behaviour_008.rs");
}
#[doc(hidden)]
mod root_tour_001 {
    use super::support::*;
    include!("tour_001.rs");
}
#[doc(hidden)]
mod root_tour_002 {
    use super::support::*;
    include!("tour_002.rs");
}
#[doc(hidden)]
mod root_tour_003 {
    use super::support::*;
    include!("tour_003.rs");
}
#[doc(hidden)]
mod root_tour_004 {
    use super::support::*;
    include!("tour_004.rs");
}
#[doc(hidden)]
mod root_tour_006 {
    use super::support::*;
    include!("tour_006.rs");
}
#[doc(hidden)]
mod root_tour_008 {
    use super::support::*;
    include!("tour_008.rs");
}
#[doc(hidden)]
mod root_tour_009 {
    use super::support::*;
    include!("tour_009.rs");
}
#[doc(hidden)]
mod root_tour_010 {
    use super::support::*;
    include!("tour_010.rs");
}
#[doc(hidden)]
mod root_tour_011 {
    use super::support::*;
    include!("tour_011.rs");
}
#[doc(hidden)]
mod root_tour_012 {
    use super::support::*;
    include!("tour_012.rs");
}
#[doc(hidden)]
mod root_tour_013 {
    use super::support::*;
    include!("tour_013.rs");
}

#[doc(hidden)]
mod front {
    use super::support::*;
    include!("front.rs");
}

#[doc(hidden)]
mod root_5_response_005 {
    use super::support::*;
    include!("5-response_005.rs");
}

#[doc(hidden)]
mod root_5_response_007 {
    use super::support::*;
    include!("5-response_007.rs");
}

#[doc(hidden)]
mod root_tour_005 {
    use super::support::*;
    include!("tour_005.rs");
}

#[doc(hidden)]
mod root_tour_007 {
    use super::support::*;
    include!("tour_007.rs");
}

#[cfg(test)]
mod repaired {
    use super::*;
    #[test]
    fn c1_20_p46_the_four_repaired_page_roots_compile_and_build_their_calls() {
        use support::*;
        let stripe = root_tour_005::Stripe::new(Secret::new("fixture"));
        assert_eq!(
            stripe.customers().list().request().unwrap().url().path(),
            "/v1/customers"
        );
        assert_eq!(
            stripe.invoices().list().request().unwrap().url().path(),
            "/v1/invoices"
        );
        root_tour_007::Fixture::new()
            .create(NewDoc("doc".into()))
            .request()
            .unwrap();
        root_5_response_005::Fixture::new()
            .get(DocId("id".into()))
            .request()
            .unwrap();
        root_5_response_007::Fixture::new()
            .create(NewTask("task".into()))
            .request()
            .unwrap();
    }
}
