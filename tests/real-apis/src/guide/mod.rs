//! Compiled descriptions from the current documentation.
#![allow(dead_code, unused_imports)]
pub mod support;
#[doc(hidden)]
mod readme_001 {
    use super::support::*;
    include!("readme_001.rs");
    pub(super) fn compiled() -> &'static str {
        ::std::any::type_name::<Tasks>()
    }
}
#[doc(hidden)]
mod readme_002 {
    use super::support::*;
    include!("readme_002.rs");
    pub(super) fn compiled() -> &'static str {
        ::std::any::type_name::<Tasks>()
    }
}
#[doc(hidden)]
mod reference_overview_001 {
    use super::support::*;
    include!("reference_overview_001.rs");
    pub(super) fn compiled() -> &'static str {
        ::std::any::type_name::<Tasks>()
    }
}
#[doc(hidden)]
mod guide_credentials_001 {
    use super::support::*;
    include!("guide_credentials_001.rs");
    pub(super) fn compiled() -> &'static str {
        ::std::any::type_name::<Vault>()
    }
}
#[doc(hidden)]
mod guide_describing_an_api_001 {
    use super::support::*;
    include!("guide_describing-an-api_001.rs");
    pub(super) fn compiled() -> &'static str {
        ::std::any::type_name::<Riot>()
    }
}
#[doc(hidden)]
mod guide_describing_an_api_002 {
    use super::support::*;
    include!("guide_describing-an-api_002.rs");
    pub(super) fn compiled() -> &'static str {
        ::std::any::type_name::<Stripe>()
    }
}
#[doc(hidden)]
mod guide_describing_an_api_003 {
    use super::support::*;
    include!("guide_describing-an-api_003.rs");
    pub(super) fn compiled() -> &'static str {
        ::std::any::type_name::<Vault>()
    }
}
#[doc(hidden)]
mod guide_testing_001 {
    use super::support::*;
    include!("guide_testing_001.rs");
    pub(super) fn compiled() -> &'static str {
        ::std::any::type_name::<Stations>()
    }
}
#[doc(hidden)]
mod guide_tour_001 {
    use super::support::*;
    include!("guide_tour_001.rs");
    pub(super) fn compiled() -> &'static str {
        ::std::any::type_name::<Tasks>()
    }
}
#[doc(hidden)]
mod guide_tour_002 {
    use super::support::*;
    include!("guide_tour_002.rs");
    pub(super) fn compiled() -> &'static str {
        ::std::any::type_name::<Riot>()
    }
}
#[doc(hidden)]
mod guide_tour_003 {
    use super::support::*;
    include!("guide_tour_003.rs");
    pub(super) fn compiled() -> &'static str {
        ::std::any::type_name::<Stripe>()
    }
}
/// The concrete client types emitted by every documented description.
pub fn compiled_descriptions() -> [&'static str; 19] {
    [
        readme_001::compiled(),
        readme_002::compiled(),
        reference_overview_001::compiled(),
        guide_credentials_001::compiled(),
        guide_describing_an_api_001::compiled(),
        guide_describing_an_api_002::compiled(),
        guide_describing_an_api_003::compiled(),
        guide_testing_001::compiled(),
        guide_tour_001::compiled(),
        guide_tour_002::compiled(),
        guide_tour_003::compiled(),
        ::std::any::type_name::<crate::elasticsearch::Elasticsearch>(),
        ::std::any::type_name::<crate::github::Github>(),
        ::std::any::type_name::<crate::kubernetes::Kubernetes>(),
        ::std::any::type_name::<crate::riot::Riot>(),
        ::std::any::type_name::<crate::s3::ObjectStore>(),
        ::std::any::type_name::<crate::slack::Slack>(),
        ::std::any::type_name::<crate::stripe::Stripe>(),
        ::std::any::type_name::<crate::telegram::Telegram>(),
    ]
}
