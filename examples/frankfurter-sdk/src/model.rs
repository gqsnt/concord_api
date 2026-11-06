use serde::{Deserialize, Deserializer, de::Error as _};
use serde_json::value::RawValue;

use crate::{Date, Decimal};

/// A Frankfurter v2 pair observation. Additional provider fields are ignored.
///
/// This includes nested attribution such as the shared wire schema's `providers` field;
/// it is not retained or used to recalculate `rate`. The optional request filter of the
/// same name is independent. Unknown values must still be valid JSON, and the four
/// known fields keep their usual validation.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct Rate {
    /// Provider's observation date, which need not equal the requested date.
    pub date: Date,
    /// Base currency or accounting unit code, as returned by the provider.
    pub base: String,
    /// Quote currency or accounting unit code, as returned by the provider.
    pub quote: String,
    /// Strictly positive JSON number parsed directly into an exact decimal.
    ///
    /// Integer, fraction and exponent spellings are accepted. No `f32`/`f64` conversion
    /// or rounding takes place. Quoted numbers, zero and negative rates are refused.
    /// Decimal scale must fit BigDecimal's signed 64-bit scale; lexical spelling is not
    /// promised (for example `1e0` and `1.0` have the same numeric value).
    #[serde(deserialize_with = "decimal")]
    pub rate: Decimal,
}

fn decimal<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Decimal, D::Error> {
    // RawValue validates JSON syntax and preserves every numeric digit before conversion.
    // Testing the first byte distinguishes a JSON number from strings/containers/literals;
    // decimal parsing itself is delegated to BigDecimal, not reimplemented here.
    let raw = Box::<RawValue>::deserialize(deserializer)?;
    if !matches!(raw.get().as_bytes().first(), Some(b'-' | b'0'..=b'9')) {
        return Err(D::Error::custom("rate must be a JSON number"));
    }
    let value: Decimal = raw
        .get()
        .parse()
        .map_err(|_| D::Error::custom("rate is outside the decimal representation"))?;
    if value <= 0 {
        return Err(D::Error::custom("rate must be positive"));
    }
    Ok(value)
}
