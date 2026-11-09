#![doc = include_str!("../README.md")]

mod csv;
mod error;
mod model;

use concord::reqwest;
use concord::{Decode, Json};

pub use concord::ResponseMeta;
pub use csv::{CSV_RECORD_LIMIT, CsvColumn, CsvForecast, CsvRow};
pub use error::{Error, ErrorBodyError, Problem, ProviderError, ResponseErrorKind};
pub use model::{Forecast, ForecastQuery, Hourly, HourlyVariable, QueryError, ResponseFormat};

/// Maximum bytes read for one successful forecast (4 MiB); larger responses fail.
pub const FORECAST_BODY_LIMIT: usize = 4 * 1024 * 1024;
/// Maximum bytes read for a non-2xx error document (16 KiB).
pub const ERROR_BODY_LIMIT: usize = 16 * 1024;

mod wire {
    use super::ResponseFormat;

    concord::api! {
        pub client ForecastApi {
            base "https://api.open-meteo.com";
        error crate::Problem in concord::Json;

            GET forecast(
                query latitude: f64,
                query longitude: f64,
                query hourly: String,
                query format: ResponseFormat,
                header("accept") accept: String,
                query timezone?: String,
                query forecast_days?: u8
            ) -> stream {
                path "v1/forecast";
            }
        }
    }
}

/// Public, unauthenticated forecast client.
///
/// Construct with [`OpenMeteo::new`] and a configured `concord::reqwest::Client`.
/// Timeouts, redirect policy, proxies and connection pooling belong to that transport.
/// The public API needs no key. Reserved commercial resources are outside this slice.
#[derive(Clone, Debug)]
pub struct OpenMeteo {
    api: wire::ForecastApi,
}

impl OpenMeteo {
    /// Use the public Open-Meteo origin and the caller's transport configuration.
    pub fn new(http: reqwest::Client) -> Self {
        Self {
            api: wire::ForecastApi::new(http),
        }
    }

    /// Replace the origin, for example with the loopback test server.
    /// A path prefix in the base URL is retained. No request is sent.
    pub fn with_base_url(mut self, base: &str) -> Result<Self, concord::Error> {
        self.api = self.api.with_base_url(base)?;
        Ok(self)
    }

    /// Sends `accept: application/json` for `ResponseFormat::Json` or
    /// `accept: text/csv` for `ResponseFormat::Csv`, from `ResponseFormat::accept()`.
    ///
    /// Retrieve one location's hourly forecast, selecting JSON or CSV at runtime.
    ///
    /// Both choices use the same declared GET operation and one execution. The chosen
    /// `format` query and `Accept` header agree; the response must have the matching media
    /// type. `Ok` means the complete bounded body and its structure were validated. No
    /// retries, pagination, background reader or partial result are hidden in this call.
    pub async fn forecast(
        &self,
        query: &ForecastQuery,
        format: ResponseFormat,
    ) -> Result<ForecastResponse, Error> {
        let mut operation = self.api.forecast(
            query.latitude,
            query.longitude,
            query
                .hourly
                .iter()
                .map(HourlyVariable::as_str)
                .collect::<Vec<_>>()
                .join(","),
            format,
            format.accept(),
        );
        if let Some(timezone) = &query.timezone {
            operation = operation.set_timezone(timezone);
        }
        if let Some(days) = query.forecast_days {
            operation = operation.set_forecast_days(days);
        }
        let received = operation
            .prepare()?
            .error_body_limit(crate::ERROR_BODY_LIMIT)
            .receive()
            .await?;
        if !received.status().is_success() {
            let (meta, document) = received.error_document(ERROR_BODY_LIMIT).await;
            let document = document.map_err(ErrorBodyError::from);
            return Err(Error::Provider(Box::new(ProviderError { meta, document })));
        }
        let meta = received.meta();
        let media = match format {
            ResponseFormat::Json => {
                <Json as Decode<Forecast>>::check_content_type(&received.head())
            }
            ResponseFormat::Csv => csv::check_head(&received.head()),
        };
        media.map_err(|source| Error::response(ResponseErrorKind::Media, source, &meta))?;
        let bytes = received.bytes_limited(FORECAST_BODY_LIMIT).await?;
        let data = match format {
            ResponseFormat::Json => {
                let forecast = <Json as Decode<Forecast>>::decode_body((), bytes)
                    .and_then(|forecast| {
                        model::validate(&forecast, query)?;
                        Ok(forecast)
                    })
                    .map_err(|source| Error::response(ResponseErrorKind::Json, source, &meta))?;
                ForecastData::Json(forecast)
            }
            ResponseFormat::Csv => ForecastData::Csv(
                csv::decode(bytes, query)
                    .map_err(|source| Error::response(ResponseErrorKind::Csv, source, &meta))?,
            ),
        };
        Ok(ForecastResponse { meta, data })
    }
}

/// A complete response and the actual head captured before reading its body.
#[derive(Debug, Clone)]
pub struct ForecastResponse {
    /// Status, HTTP version, headers (including Content-Type), and operation identity.
    pub meta: ResponseMeta,
    /// The representation explicitly selected by the caller.
    pub data: ForecastData,
}

/// Runtime response representation of the single forecast operation.
#[derive(Debug, Clone)]
pub enum ForecastData {
    /// Typed JSON forecast, preserving units and additional hourly variables.
    Json(Forecast),
    /// Original CSV bytes, location metadata, units and mapped runtime rows.
    Csv(CsvForecast),
}
