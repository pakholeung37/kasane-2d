//! Experimental CBOR project codec. The production project format remains JSON.

use kasane_core::types::Status;
use kasane_core::Document;

use super::decode::decode_wire;
use super::encode::encode_wire;
use super::types::ProjectWire;

const MAGIC: &[u8; 8] = b"KASCBOR1";
const HEADER_LEN: usize = 16;

pub fn encode_project_cbor(document: &Document) -> Result<Vec<u8>, Status> {
    let wire = encode_wire(document)?;
    let mut bytes = Vec::new();
    bytes.extend_from_slice(MAGIC);
    bytes.extend_from_slice(&[0; 8]);
    ciborium::into_writer(&wire, &mut bytes)
        .map_err(|error| Status::error("INVALID_PROJECT", error.to_string()))?;
    let payload_len = (bytes.len() - HEADER_LEN) as u64;
    bytes[MAGIC.len()..HEADER_LEN].copy_from_slice(&payload_len.to_le_bytes());
    Ok(bytes)
}

pub fn decode_project_cbor(bytes: &[u8]) -> Result<Document, Status> {
    if bytes.len() < HEADER_LEN || &bytes[..MAGIC.len()] != MAGIC {
        return Err(Status::error(
            "INVALID_PROJECT",
            "Invalid binary project header",
        ));
    }
    let declared_len = u64::from_le_bytes(bytes[MAGIC.len()..HEADER_LEN].try_into().unwrap());
    if declared_len != (bytes.len() - HEADER_LEN) as u64 {
        return Err(Status::error(
            "INVALID_PROJECT",
            "Binary project length differs from header",
        ));
    }
    let wire: ProjectWire = ciborium::from_reader(&bytes[HEADER_LEN..])
        .map_err(|error| Status::error("INVALID_PROJECT", error.to_string()))?;
    decode_wire(wire)
}
