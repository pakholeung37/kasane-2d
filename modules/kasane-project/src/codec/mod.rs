#[cfg(feature = "binary-prototype")]
mod binary_prototype;
mod decode;
mod encode;
mod types;
mod validation;

#[cfg(feature = "binary-prototype")]
pub use binary_prototype::{decode_project_cbor, encode_project_cbor};
pub use decode::decode_project;
pub use encode::encode_project;
pub use validation::validate_json_syntax;
