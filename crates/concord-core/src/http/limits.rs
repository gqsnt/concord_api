/// Maximum byte length of one proxy URL supplied for structural validation.
pub const MAX_PROXY_URL_BYTES: usize = 8_192;

/// Maximum number of retained custom root-certificate PEM inputs.
pub const MAX_ROOT_CERTIFICATES: usize = 64;

/// Maximum byte length of one retained custom root-certificate PEM input.
pub const MAX_ROOT_CERTIFICATE_BYTES: usize = 1_048_576;

/// Maximum aggregate bytes retained across custom root-certificate PEM inputs.
pub const MAX_ROOT_CERTIFICATES_TOTAL_BYTES: usize = 16_777_216;

/// Maximum byte length of the retained application identity PEM input.
pub const MAX_IDENTITY_PEM_BYTES: usize = 4_194_304;
