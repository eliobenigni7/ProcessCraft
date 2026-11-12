//! Bounded, streaming projection of serialized model scalars into JSON pointers.
use crate::queries::ModelField;
use serde::ser::Error as _;
use serde::ser::{
    self, Serialize, SerializeMap, SerializeSeq, SerializeStruct, SerializeStructVariant,
    SerializeTuple, SerializeTupleStruct, SerializeTupleVariant,
};
use serde_json::Value;
use std::fmt;

const MAX_BYTES: usize = 3 * 1024 * 1024;
#[derive(Debug)]
pub struct ProjectionError(pub String);
impl fmt::Display for ProjectionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for ProjectionError {}
impl ser::Error for ProjectionError {
    fn custom<T: fmt::Display>(msg: T) -> Self {
        Self(msg.to_string())
    }
}
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FieldPage {
    pub total: usize,
    pub items: Vec<ModelField>,
    pub next_offset: Option<usize>,
}
struct State {
    offset: usize,
    limit: usize,
    total: usize,
    bytes: usize,
    items: Vec<ModelField>,
    full: bool,
}
impl State {
    fn emit(&mut self, path: &str, value: impl FnOnce() -> Value) -> Result<(), ProjectionError> {
        let row = self.total;
        self.total = self
            .total
            .checked_add(1)
            .ok_or_else(|| ProjectionError("Projection row count overflow".into()))?;
        if row < self.offset || self.items.len() >= self.limit || self.full {
            return Ok(());
        }
        let field = ModelField {
            path: path.into(),
            value: value(),
        };
        struct Counter(usize);
        impl std::io::Write for Counter {
            fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
                self.0 = self
                    .0
                    .checked_add(bytes.len())
                    .ok_or_else(|| std::io::Error::other("Projection size overflow"))?;
                Ok(bytes.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        let mut counter = Counter(0);
        serde_json::to_writer(&mut counter, &field).map_err(ProjectionError::custom)?;
        let size = counter
            .0
            .checked_add(1)
            .ok_or_else(|| ProjectionError("Projection size overflow".into()))?;
        if self.bytes.saturating_add(size) > MAX_BYTES - 1024 {
            if self.items.is_empty() {
                return Err(ProjectionError(
                    "First projected field exceeds the 3 MiB response budget".into(),
                ));
            }
            self.full = true;
            return Ok(());
        }
        self.bytes += size;
        self.items.push(field);
        Ok(())
    }
}
/// Traverses all rows for an exact count, retaining only the bounded requested page.
pub fn project_fields_page<T: Serialize + ?Sized>(
    value: &T,
    offset: usize,
    limit: usize,
) -> Result<FieldPage, ProjectionError> {
    if !(1..=100).contains(&limit) {
        return Err(ProjectionError(
            "Projection limit must be between 1 and 100".into(),
        ));
    }
    let mut state = State {
        offset,
        limit,
        total: 0,
        bytes: 0,
        items: Vec::new(),
        full: false,
    };
    value.serialize(Projector {
        state: &mut state,
        path: String::new(),
    })?;
    let next = offset.saturating_add(state.items.len());
    Ok(FieldPage {
        total: state.total,
        items: state.items,
        next_offset: (next < state.total).then_some(next),
    })
}
fn child(path: &str, key: &str) -> String {
    format!("{}/{}", path, key.replace('~', "~0").replace('/', "~1"))
}
struct Projector<'a> {
    state: &'a mut State,
    path: String,
}
struct Container<'a> {
    state: &'a mut State,
    path: String,
    count: usize,
    map: bool,
    key: Option<String>,
}
impl<'a> Container<'a> {
    fn entry<T: Serialize + ?Sized>(
        &mut self,
        key: &str,
        value: &T,
    ) -> Result<(), ProjectionError> {
        self.count += 1;
        value.serialize(Projector {
            state: self.state,
            path: child(&self.path, key),
        })
    }
    fn finish(self) -> Result<(), ProjectionError> {
        if self.count == 0 {
            self.state.emit(&self.path, || {
                if self.map {
                    Value::Object(Default::default())
                } else {
                    Value::Array(vec![])
                }
            })?;
        }
        Ok(())
    }
}
macro_rules! scalar {
    ($name:ident,$type:ty) => {
        fn $name(self, v: $type) -> Result<(), ProjectionError> {
            self.state.emit(&self.path, || {
                serde_json::to_value(v).unwrap_or(Value::Null)
            })
        }
    };
}
impl<'a> ser::Serializer for Projector<'a> {
    type Ok = ();
    type Error = ProjectionError;
    type SerializeSeq = Container<'a>;
    type SerializeTuple = Container<'a>;
    type SerializeTupleStruct = Container<'a>;
    type SerializeTupleVariant = Container<'a>;
    type SerializeMap = Container<'a>;
    type SerializeStruct = Container<'a>;
    type SerializeStructVariant = Container<'a>;
    scalar!(serialize_bool, bool);
    scalar!(serialize_i8, i8);
    scalar!(serialize_i16, i16);
    scalar!(serialize_i32, i32);
    scalar!(serialize_i64, i64);
    fn serialize_i128(self, v: i128) -> Result<(), ProjectionError> {
        let n = serde_json::Number::from_i128(v)
            .ok_or_else(|| ProjectionError("Integer outside JSON number range".into()))?;
        self.state.emit(&self.path, || Value::Number(n))
    }
    scalar!(serialize_u8, u8);
    scalar!(serialize_u16, u16);
    scalar!(serialize_u32, u32);
    scalar!(serialize_u64, u64);
    fn serialize_u128(self, v: u128) -> Result<(), ProjectionError> {
        let n = serde_json::Number::from_u128(v)
            .ok_or_else(|| ProjectionError("Integer outside JSON number range".into()))?;
        self.state.emit(&self.path, || Value::Number(n))
    }
    scalar!(serialize_f32, f32);
    scalar!(serialize_f64, f64);
    scalar!(serialize_char, char);
    fn serialize_str(self, v: &str) -> Result<(), ProjectionError> {
        if v.len().saturating_add(self.path.len()) > MAX_BYTES
            && self.state.total >= self.state.offset
            && self.state.items.len() < self.state.limit
            && !self.state.full
        {
            self.state.total = self
                .state
                .total
                .checked_add(1)
                .ok_or_else(|| ProjectionError("Projection row count overflow".into()))?;
            if self.state.items.is_empty() {
                return Err(ProjectionError(
                    "First projected field exceeds the 3 MiB response budget".into(),
                ));
            }
            self.state.full = true;
            return Ok(());
        }
        self.state.emit(&self.path, || Value::String(v.to_owned()))
    }
    fn serialize_bytes(self, v: &[u8]) -> Result<(), ProjectionError> {
        let mut c = self.serialize_seq(Some(v.len()))?;
        for b in v {
            SerializeSeq::serialize_element(&mut c, b)?;
        }
        SerializeSeq::end(c)
    }
    fn serialize_none(self) -> Result<(), ProjectionError> {
        self.serialize_unit()
    }
    fn serialize_some<T: Serialize + ?Sized>(self, v: &T) -> Result<(), ProjectionError> {
        v.serialize(self)
    }
    fn serialize_unit(self) -> Result<(), ProjectionError> {
        self.state.emit(&self.path, || Value::Null)
    }
    fn serialize_unit_struct(self, _: &'static str) -> Result<(), ProjectionError> {
        self.serialize_unit()
    }
    fn serialize_unit_variant(
        self,
        _: &'static str,
        _: u32,
        v: &'static str,
    ) -> Result<(), ProjectionError> {
        self.serialize_str(v)
    }
    fn serialize_newtype_struct<T: Serialize + ?Sized>(
        self,
        _: &'static str,
        v: &T,
    ) -> Result<(), ProjectionError> {
        v.serialize(self)
    }
    fn serialize_newtype_variant<T: Serialize + ?Sized>(
        self,
        _: &'static str,
        _: u32,
        name: &'static str,
        v: &T,
    ) -> Result<(), ProjectionError> {
        v.serialize(Projector {
            state: self.state,
            path: child(&self.path, name),
        })
    }
    fn serialize_seq(self, _: Option<usize>) -> Result<Container<'a>, ProjectionError> {
        Ok(Container {
            state: self.state,
            path: self.path,
            count: 0,
            map: false,
            key: None,
        })
    }
    fn serialize_tuple(self, _: usize) -> Result<Container<'a>, ProjectionError> {
        self.serialize_seq(None)
    }
    fn serialize_tuple_struct(
        self,
        _: &'static str,
        _: usize,
    ) -> Result<Container<'a>, ProjectionError> {
        self.serialize_seq(None)
    }
    fn serialize_tuple_variant(
        self,
        _: &'static str,
        _: u32,
        name: &'static str,
        _: usize,
    ) -> Result<Container<'a>, ProjectionError> {
        Ok(Container {
            state: self.state,
            path: child(&self.path, name),
            count: 0,
            map: false,
            key: None,
        })
    }
    fn serialize_map(self, _: Option<usize>) -> Result<Container<'a>, ProjectionError> {
        Ok(Container {
            state: self.state,
            path: self.path,
            count: 0,
            map: true,
            key: None,
        })
    }
    fn serialize_struct(self, _: &'static str, _: usize) -> Result<Container<'a>, ProjectionError> {
        self.serialize_map(None)
    }
    fn serialize_struct_variant(
        self,
        _: &'static str,
        _: u32,
        name: &'static str,
        _: usize,
    ) -> Result<Container<'a>, ProjectionError> {
        Ok(Container {
            state: self.state,
            path: child(&self.path, name),
            count: 0,
            map: true,
            key: None,
        })
    }
}
impl SerializeSeq for Container<'_> {
    type Ok = ();
    type Error = ProjectionError;
    fn serialize_element<T: Serialize + ?Sized>(&mut self, v: &T) -> Result<(), ProjectionError> {
        self.entry(&self.count.to_string(), v)
    }
    fn end(self) -> Result<(), ProjectionError> {
        self.finish()
    }
}
impl SerializeTuple for Container<'_> {
    type Ok = ();
    type Error = ProjectionError;
    fn serialize_element<T: Serialize + ?Sized>(&mut self, v: &T) -> Result<(), ProjectionError> {
        SerializeSeq::serialize_element(self, v)
    }
    fn end(self) -> Result<(), ProjectionError> {
        self.finish()
    }
}
impl SerializeTupleStruct for Container<'_> {
    type Ok = ();
    type Error = ProjectionError;
    fn serialize_field<T: Serialize + ?Sized>(&mut self, v: &T) -> Result<(), ProjectionError> {
        SerializeSeq::serialize_element(self, v)
    }
    fn end(self) -> Result<(), ProjectionError> {
        self.finish()
    }
}
impl SerializeTupleVariant for Container<'_> {
    type Ok = ();
    type Error = ProjectionError;
    fn serialize_field<T: Serialize + ?Sized>(&mut self, v: &T) -> Result<(), ProjectionError> {
        SerializeSeq::serialize_element(self, v)
    }
    fn end(self) -> Result<(), ProjectionError> {
        self.finish()
    }
}
impl SerializeMap for Container<'_> {
    type Ok = ();
    type Error = ProjectionError;
    fn serialize_key<T: Serialize + ?Sized>(&mut self, key: &T) -> Result<(), ProjectionError> {
        let v = serde_json::to_value(key).map_err(ProjectionError::custom)?;
        self.key = Some(match v {
            Value::String(s) => s,
            Value::Number(n) => n.to_string(),
            Value::Bool(b) => b.to_string(),
            _ => return Err(ProjectionError("Invalid JSON object key".into())),
        });
        Ok(())
    }
    fn serialize_value<T: Serialize + ?Sized>(&mut self, v: &T) -> Result<(), ProjectionError> {
        let k = self
            .key
            .take()
            .ok_or_else(|| ProjectionError("Missing map key".into()))?;
        self.entry(&k, v)
    }
    fn end(self) -> Result<(), ProjectionError> {
        self.finish()
    }
}
impl SerializeStruct for Container<'_> {
    type Ok = ();
    type Error = ProjectionError;
    fn serialize_field<T: Serialize + ?Sized>(
        &mut self,
        k: &'static str,
        v: &T,
    ) -> Result<(), ProjectionError> {
        self.entry(k, v)
    }
    fn end(self) -> Result<(), ProjectionError> {
        self.finish()
    }
}
impl SerializeStructVariant for Container<'_> {
    type Ok = ();
    type Error = ProjectionError;
    fn serialize_field<T: Serialize + ?Sized>(
        &mut self,
        k: &'static str,
        v: &T,
    ) -> Result<(), ProjectionError> {
        self.entry(k, v)
    }
    fn end(self) -> Result<(), ProjectionError> {
        self.finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::ser::SerializeSeq;
    use std::cell::Cell;
    use std::collections::BTreeMap;

    #[test]
    fn pointers_escape_keys_and_include_empty_values() {
        let value = serde_json::json!({"a/b~c": {"items": [null, {}, [], "ok"]}});
        let page = project_fields_page(&value, 1, 2).unwrap();
        assert_eq!(page.total, 4);
        assert_eq!(page.next_offset, Some(3));
        assert_eq!(page.items[0].path, "/a~1b~0c/items/1");
        assert_eq!(page.items[0].value, serde_json::json!({}));
        assert_eq!(page.items[1].value, serde_json::json!([]));
    }
    #[test]
    fn pages_match_serialized_model_fields_as_a_map() {
        let doc = crate::model::BpmnDocument::new("model".into(), "definitions".parse().unwrap());
        let actual = project_fields_page(&doc, 0, 100).unwrap();
        let value = serde_json::to_value(&doc).unwrap();
        fn collect(value: &Value, path: String, out: &mut BTreeMap<String, Value>) {
            match value {
                Value::Object(map) if !map.is_empty() => {
                    for (key, value) in map {
                        collect(value, child(&path, key), out);
                    }
                }
                Value::Array(array) if !array.is_empty() => {
                    for (i, value) in array.iter().enumerate() {
                        collect(value, child(&path, &i.to_string()), out);
                    }
                }
                _ => {
                    out.insert(path, value.clone());
                }
            }
        }
        let mut expected = BTreeMap::new();
        collect(&value, String::new(), &mut expected);
        let observed: BTreeMap<_, _> = actual
            .items
            .into_iter()
            .map(|row| (row.path, row.value))
            .collect();
        assert_eq!(observed, expected);
    }
    #[test]
    fn enum_variants_preserve_json_paths_and_page_cursor() {
        #[derive(serde::Serialize)]
        enum Variant {
            Unit,
            Newtype(String),
            Tuple(u32, String),
            Struct { key: bool },
        }
        let variants = [
            Variant::Unit,
            Variant::Newtype("value".into()),
            Variant::Tuple(7, "x".into()),
            Variant::Struct { key: true },
        ];
        let page = project_fields_page(&variants, 0, 3).unwrap();
        let paths: Vec<_> = page.items.iter().map(|item| item.path.as_str()).collect();
        assert_eq!(paths, ["/0", "/1/Newtype", "/2/Tuple/0"]);
        assert_eq!(serde_json::to_value(&page).unwrap()["nextOffset"], 3);
        let last = project_fields_page(&variants, 3, 3).unwrap();
        assert_eq!(
            last.items
                .iter()
                .map(|item| item.path.as_str())
                .collect::<Vec<_>>(),
            ["/2/Tuple/1", "/3/Struct/key"]
        );
    }
    #[test]
    fn large_sequence_is_scanned_but_only_page_is_retained() {
        struct Many<'a>(&'a Cell<usize>);
        impl Serialize for Many<'_> {
            fn serialize<S: ser::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                let mut seq = serializer.serialize_seq(Some(100_000))?;
                for i in 0..100_000 {
                    self.0.set(self.0.get() + 1);
                    seq.serialize_element(&i)?;
                }
                seq.end()
            }
        }
        let count = Cell::new(0);
        let page = project_fields_page(&Many(&count), 10, 2).unwrap();
        assert_eq!(count.get(), 100_000);
        assert_eq!(page.total, 100_000);
        assert_eq!(page.items.len(), 2);
        assert_eq!(page.next_offset, Some(12));
    }
    #[test]
    fn first_oversized_field_fails_and_later_field_defers() {
        let value = vec!["ok".to_string(), "x".repeat(MAX_BYTES)];
        let first = project_fields_page(&value, 0, 10).unwrap();
        assert_eq!(first.items.len(), 1);
        assert_eq!(first.total, 2);
        assert_eq!(first.next_offset, Some(1));
        assert!(project_fields_page(&value, 1, 10).is_err());
        assert!(project_fields_page(&value, 0, 0).is_err());
    }
}
