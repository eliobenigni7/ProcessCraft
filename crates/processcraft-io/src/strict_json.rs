//! Duplicate-key detection before untrusted JSON is deserialized into maps.
use serde::de::{self, DeserializeSeed, MapAccess, SeqAccess, Visitor};
use std::{collections::HashSet, fmt};

pub fn has_duplicate_keys(bytes: &[u8]) -> bool {
    let mut deserializer = serde_json::Deserializer::from_slice(bytes);
    Check
        .deserialize(&mut deserializer)
        .is_err_and(|error| error.to_string().contains("duplicate JSON key"))
}
struct Check;

impl<'de> DeserializeSeed<'de> for Check {
    type Value = ();
    fn deserialize<D: de::Deserializer<'de>>(self, d: D) -> Result<(), D::Error> {
        d.deserialize_any(self)
    }
}
impl<'de> Visitor<'de> for Check {
    type Value = ();
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("JSON value")
    }
    fn visit_bool<E: de::Error>(self, _: bool) -> Result<(), E> {
        Ok(())
    }
    fn visit_i64<E: de::Error>(self, _: i64) -> Result<(), E> {
        Ok(())
    }
    fn visit_u64<E: de::Error>(self, _: u64) -> Result<(), E> {
        Ok(())
    }
    fn visit_f64<E: de::Error>(self, _: f64) -> Result<(), E> {
        Ok(())
    }
    fn visit_str<E: de::Error>(self, _: &str) -> Result<(), E> {
        Ok(())
    }
    fn visit_none<E: de::Error>(self) -> Result<(), E> {
        Ok(())
    }
    fn visit_unit<E: de::Error>(self) -> Result<(), E> {
        Ok(())
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut a: A) -> Result<(), A::Error> {
        while a.next_element_seed(Check)?.is_some() {}
        Ok(())
    }
    fn visit_map<A: MapAccess<'de>>(self, mut a: A) -> Result<(), A::Error> {
        let mut keys = HashSet::new();
        while let Some(k) = a.next_key::<String>()? {
            if !keys.insert(k) {
                return Err(de::Error::custom("duplicate JSON key"));
            }
            a.next_value_seed(Check)?;
        }
        Ok(())
    }
}
