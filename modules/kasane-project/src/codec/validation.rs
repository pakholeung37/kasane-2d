use std::collections::HashSet;
use std::fmt;

use kasane_core::types::Status;
use serde::de::{DeserializeSeed, Deserializer, MapAccess, SeqAccess, Visitor};

struct DuplicateKeyCheckerSeed(usize);

impl<'de> Visitor<'de> for DuplicateKeyCheckerSeed {
    type Value = ();

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("valid JSON without duplicate keys")
    }

    fn visit_map<M>(self, mut access: M) -> Result<(), M::Error>
    where
        M: MapAccess<'de>,
    {
        if self.0 > 128 {
            return Err(serde::de::Error::custom("JSON nesting exceeds 128 levels"));
        }
        let mut seen = HashSet::new();
        while let Some(key) = access.next_key::<String>()? {
            if key.contains('\0') {
                return Err(serde::de::Error::custom("Embedded NUL is not supported"));
            }
            if !seen.insert(key) {
                return Err(serde::de::Error::custom("Duplicate JSON member"));
            }
            let () = access.next_value_seed(DuplicateKeyCheckerSeed(self.0 + 1))?;
        }
        Ok(())
    }

    fn visit_seq<S>(self, mut access: S) -> Result<(), S::Error>
    where
        S: SeqAccess<'de>,
    {
        if self.0 > 128 {
            return Err(serde::de::Error::custom("JSON nesting exceeds 128 levels"));
        }
        while access
            .next_element_seed(DuplicateKeyCheckerSeed(self.0 + 1))?
            .is_some()
        {}
        Ok(())
    }

    fn visit_bool<E>(self, _: bool) -> Result<(), E> {
        Ok(())
    }

    fn visit_i64<E>(self, _: i64) -> Result<(), E> {
        Ok(())
    }

    fn visit_u64<E>(self, _: u64) -> Result<(), E> {
        Ok(())
    }

    fn visit_f64<E>(self, v: f64) -> Result<(), E>
    where
        E: serde::de::Error,
    {
        if !v.is_finite() {
            return Err(serde::de::Error::custom("Non-finite number"));
        }
        Ok(())
    }

    fn visit_str<E>(self, s: &str) -> Result<(), E>
    where
        E: serde::de::Error,
    {
        if s.contains('\0') {
            return Err(serde::de::Error::custom("Embedded NUL is not supported"));
        }
        Ok(())
    }

    fn visit_unit<E>(self) -> Result<(), E> {
        Ok(())
    }
}

impl<'de> DeserializeSeed<'de> for DuplicateKeyCheckerSeed {
    type Value = ();

    fn deserialize<D>(self, deserializer: D) -> Result<(), D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_any(self)
    }
}

pub fn validate_json_syntax(text: &str) -> Result<(), Status> {
    let mut de = serde_json::Deserializer::from_str(text);
    DuplicateKeyCheckerSeed(0)
        .deserialize(&mut de)
        .map_err(|e| Status::error("INVALID_PROJECT", e.to_string()))?;
    Ok(())
}
