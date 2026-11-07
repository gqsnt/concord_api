//! Closed observation query value sets from the FRED v1 documentation.
//! <https://fred.stlouisfed.org/docs/api/fred/series_observations.html>
//! Read 2026-11-07. No calculations or value coercion take place in the SDK.

use crate::Error;

// One mapping per option drives both validation and wire formatting.
macro_rules! query_option {
    ($(#[$doc:meta])* $name:ident, $parameter:literal {
        $($(#[$variant_doc:meta])* $variant:ident => $wire:literal),+ $(,)?
    }) => {
        $(#[$doc])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        pub enum $name {
            $($(#[$variant_doc])* $variant),+
        }

        impl $name {
            /// Validate an exact, case-sensitive provider code before building a request.
            pub fn new(value: &str) -> Result<Self, Error> {
                match value {
                    $($wire => Ok(Self::$variant),)+
                    _ => Err(Error::InvalidParameter { parameter: $parameter }),
                }
            }

            /// The documented query value.
            pub const fn as_str(self) -> &'static str {
                match self {
                    $(Self::$variant => $wire,)+
                }
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(self.as_str())
            }
        }

        impl std::str::FromStr for $name {
            type Err = Error;

            fn from_str(value: &str) -> Result<Self, Self::Err> {
                Self::new(value)
            }
        }
    };
}

query_option! {
    /// Requested value transformation. Omitting this control leaves FRED's `lin` default.
    Units, "units" {
        /// Untransformed levels (`lin`).
        Levels => "lin",
        /// Change (`chg`).
        Change => "chg",
        /// Change from year ago (`ch1`).
        ChangeFromYearAgo => "ch1",
        /// Percent change (`pch`).
        PercentChange => "pch",
        /// Percent change from year ago (`pc1`).
        PercentChangeFromYearAgo => "pc1",
        /// Compounded annual rate of change (`pca`).
        CompoundedAnnualRateOfChange => "pca",
        /// Continuously compounded rate of change (`cch`).
        ContinuouslyCompoundedRateOfChange => "cch",
        /// Continuously compounded annual rate of change (`cca`).
        ContinuouslyCompoundedAnnualRateOfChange => "cca",
        /// Natural logarithm (`log`).
        NaturalLog => "log",
    }
}

query_option! {
    /// Requested frequency; omitting it leaves the native series frequency.
    /// Native-frequency compatibility is checked by FRED. Its documentation
    /// limits frequency aggregation to XML/JSON; CSV only forwards this query.
    Frequency, "frequency" {
        /// Daily (`d`).
        Daily => "d",
        /// Weekly, with the provider's implicit period (`w`).
        Weekly => "w",
        /// Biweekly, with the provider's implicit period (`bw`).
        Biweekly => "bw",
        /// Monthly (`m`).
        Monthly => "m",
        /// Quarterly (`q`).
        Quarterly => "q",
        /// Semiannual (`sa`).
        Semiannual => "sa",
        /// Annual (`a`).
        Annual => "a",
        /// Weekly, ending Friday (`wef`).
        WeeklyEndingFriday => "wef",
        /// Weekly, ending Thursday (`weth`).
        WeeklyEndingThursday => "weth",
        /// Weekly, ending Wednesday (`wew`).
        WeeklyEndingWednesday => "wew",
        /// Weekly, ending Tuesday (`wetu`).
        WeeklyEndingTuesday => "wetu",
        /// Weekly, ending Monday (`wem`).
        WeeklyEndingMonday => "wem",
        /// Weekly, ending Sunday (`wesu`).
        WeeklyEndingSunday => "wesu",
        /// Weekly, ending Saturday (`wesa`).
        WeeklyEndingSaturday => "wesa",
        /// Biweekly, ending Wednesday (`bwew`).
        BiweeklyEndingWednesday => "bwew",
        /// Biweekly, ending Monday (`bwem`).
        BiweeklyEndingMonday => "bwem",
    }
}

query_option! {
    /// Frequency aggregation method. FRED defaults to `avg` and ignores this
    /// parameter when no frequency is set. The SDK does not aggregate locally.
    AggregationMethod, "aggregation_method" {
        /// Average (`avg`).
        Average => "avg",
        /// Sum (`sum`).
        Sum => "sum",
        /// End of period (`eop`).
        EndOfPeriod => "eop",
    }
}
