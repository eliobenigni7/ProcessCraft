use std::{fmt, str::FromStr};

use uuid::Uuid;

/// An identity must be a well-formed, non-nil UUID.
#[derive(Debug)]
pub enum InvalidId {
    Malformed(uuid::Error),
    Nil,
}

impl fmt::Display for InvalidId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Malformed(error) => write!(f, "Invalid UUID: {error}"),
            Self::Nil => f.write_str("An identity cannot be the nil UUID"),
        }
    }
}

impl std::error::Error for InvalidId {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Malformed(error) => Some(error),
            Self::Nil => None,
        }
    }
}

macro_rules! identity {
    ($name:ident, $description:literal) => {
        #[doc = $description]
        #[derive(
            Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, schemars::JsonSchema,
        )]
        #[schemars(with = "String")]
        pub struct $name(Uuid);

        impl $name {
            pub fn new() -> Self {
                Self(Uuid::new_v4())
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }

        impl FromStr for $name {
            type Err = InvalidId;

            fn from_str(text: &str) -> Result<Self, Self::Err> {
                let uuid = Uuid::parse_str(text).map_err(InvalidId::Malformed)?;
                if uuid.is_nil() {
                    return Err(InvalidId::Nil);
                }
                Ok(Self(uuid))
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0.fmt(f)
            }
        }
    };
}

identity!(CommentId, "Stable identity of a comment.");
identity!(CheckpointId, "Stable identity of a checkpoint.");
identity!(DictionaryEntryId, "Stable identity of a dictionary entry.");
identity!(ProjectId, "Stable identity of a project.");
identity!(
    NativeElementId,
    "Stable native identity of a BPMN element or representation."
);
identity!(
    DocumentId,
    "Stable identity of a document within a project."
);

#[cfg(test)]
mod tests {
    use super::{DocumentId, ProjectId};

    #[test]
    fn identifiers_round_trip_without_changing_identity() {
        let text = "0730c00d-df2d-4676-97c9-96033534719e";
        let project: ProjectId = text.parse().unwrap();
        let document: DocumentId = text.parse().unwrap();
        assert_eq!(project.to_string(), text);
        assert_eq!(document.to_string(), text);
        assert_eq!(project.to_string().parse::<ProjectId>().unwrap(), project);
        assert_eq!(
            document.to_string().parse::<DocumentId>().unwrap(),
            document
        );
    }

    #[test]
    fn identifiers_reject_malformed_and_nil_uuids() {
        for text in [
            "",
            "project-1",
            "0730c00d-df2d-4676-97c9-96033534719z",
            "00000000-0000-0000-0000-000000000000",
        ] {
            assert!(text.parse::<ProjectId>().is_err(), "accepted {text:?}");
            assert!(text.parse::<DocumentId>().is_err(), "accepted {text:?}");
        }
    }

    #[test]
    fn generated_identifiers_are_valid_and_distinct() {
        let project = ProjectId::new();
        let document = DocumentId::new();
        assert_eq!(project.to_string().parse::<ProjectId>().unwrap(), project);
        assert_eq!(
            document.to_string().parse::<DocumentId>().unwrap(),
            document
        );
        assert_ne!(project, ProjectId::new());
        assert_ne!(document, DocumentId::new());
    }
}

#[cfg(test)]
mod element_identity_tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn legacy_json_and_native_identity_are_stable() {
        let legacy: ElementIdentity = serde_json::from_str("\"Task_1\"").unwrap();
        assert_eq!(serde_json::to_string(&legacy).unwrap(), "\"Task_1\"");
        assert_eq!(
            legacy.selector(),
            ElementSelector::Xml("Task_1".parse().unwrap())
        );
        let native_id = NativeElementId::new();
        let before = ElementIdentity::Native {
            native_id,
            xml_id: None,
        };
        let after = ElementIdentity::Native {
            native_id,
            xml_id: Some("Task_2".parse().unwrap()),
        };
        assert_eq!(before, after);
        assert_eq!(HashSet::from([before.clone(), after.clone()]).len(), 1);
        assert_ne!(
            serde_json::to_string(&before).unwrap(),
            serde_json::to_string(&after).unwrap()
        );
        let roundtrip: ElementIdentity =
            serde_json::from_str(&serde_json::to_string(&after).unwrap()).unwrap();
        assert_eq!(roundtrip.xml_id(), after.xml_id());
        assert_eq!(roundtrip.selector(), after.selector());
        assert!(
            serde_json::from_str::<ElementIdentity>(
                "{\"native_id\":\"00000000-0000-0000-0000-000000000000\",\"xml_id\":null}"
            )
            .is_err()
        );
        assert!(
            serde_json::from_str::<ElementSelector>(
                "{\"native_id\":\"00000000-0000-0000-0000-000000000000\"}"
            )
            .is_err()
        );
        let selector = after.selector();
        assert_eq!(
            selector.to_string().parse::<ElementSelector>().unwrap(),
            selector
        );
        assert!(
            "native:00000000-0000-0000-0000-000000000000"
                .parse::<ElementSelector>()
                .is_err()
        );
        assert!("native:not-a-uuid".parse::<ElementSelector>().is_err());
    }
}
use serde::{Deserialize, Deserializer, Serialize, Serializer};

macro_rules! serde_identity {
    ($id:ty) => {
        impl Serialize for $id {
            fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
                s.serialize_str(&self.to_string())
            }
        }
        impl<'de> Deserialize<'de> for $id {
            fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
                String::deserialize(d)?
                    .parse()
                    .map_err(serde::de::Error::custom)
            }
        }
    };
}
serde_identity!(CommentId);
serde_identity!(CheckpointId);
serde_identity!(NativeElementId);
impl Serialize for DictionaryEntryId {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
    }
}
impl<'de> Deserialize<'de> for DictionaryEntryId {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        String::deserialize(d)?
            .parse()
            .map_err(serde::de::Error::custom)
    }
}
impl Serialize for ProjectId {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
    }
}
impl<'de> Deserialize<'de> for ProjectId {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        String::deserialize(d)?
            .parse()
            .map_err(serde::de::Error::custom)
    }
}
impl Serialize for DocumentId {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
    }
}
impl<'de> Deserialize<'de> for DocumentId {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        String::deserialize(d)?
            .parse()
            .map_err(serde::de::Error::custom)
    }
}

/// An XML NCName, preserved exactly as supplied.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, schemars::JsonSchema)]
#[schemars(with = "String")]
pub struct XmlId(String);
fn name_start(c: char) -> bool {
    matches!(c, 'A'..='Z'|'a'..='z'|'_'|'\u{c0}'..='\u{d6}'|'\u{d8}'..='\u{f6}'|'\u{f8}'..='\u{2ff}'|'\u{370}'..='\u{37d}'|'\u{37f}'..='\u{1fff}'|'\u{200c}'..='\u{200d}'|'\u{2070}'..='\u{218f}'|'\u{2c00}'..='\u{2fef}'|'\u{3001}'..='\u{d7ff}'|'\u{f900}'..='\u{fdcf}'|'\u{fdf0}'..='\u{fffd}'|'\u{10000}'..='\u{effff}')
}
impl XmlId {
    pub(crate) fn allocated_bytes(&self) -> usize {
        self.0.capacity()
    }
}
impl FromStr for XmlId {
    type Err = &'static str;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut chars = s.chars();
        if s.len()>16384 || !chars.next().is_some_and(name_start) || !chars.all(|c| name_start(c) || matches!(c,'-'|'.'|'0'..='9'|'\u{b7}'|'\u{300}'..='\u{36f}'|'\u{203f}'..='\u{2040}')) { return Err("invalid XML NCName"); }
        Ok(Self(s.to_owned()))
    }
}
impl fmt::Display for XmlId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}
impl<'de> Deserialize<'de> for XmlId {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        String::deserialize(d)?
            .parse()
            .map_err(serde::de::Error::custom)
    }
}
/// Persistent element identity; legacy XML IDs remain serialized as strings.
#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(untagged, deny_unknown_fields)]
pub enum ElementIdentity {
    Legacy(XmlId),
    Native {
        native_id: NativeElementId,
        xml_id: Option<XmlId>,
    },
}
/// A native or legacy selector. External XML QNames are separate model values.
#[derive(
    Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(untagged, deny_unknown_fields)]
pub enum ElementSelector {
    Xml(XmlId),
    Native { native_id: NativeElementId },
}
impl ElementIdentity {
    pub fn anonymous() -> Self {
        Self::Native {
            native_id: NativeElementId::new(),
            xml_id: None,
        }
    }
    pub fn xml_id(&self) -> Option<&XmlId> {
        match self {
            Self::Legacy(id) => Some(id),
            Self::Native { xml_id, .. } => xml_id.as_ref(),
        }
    }
    pub fn selector(&self) -> ElementSelector {
        match self {
            Self::Legacy(id) => ElementSelector::Xml(id.clone()),
            Self::Native { native_id, .. } => ElementSelector::Native {
                native_id: *native_id,
            },
        }
    }
}
impl ElementSelector {
    pub fn is_native(&self) -> bool {
        matches!(self, Self::Native { .. })
    }
}
impl FromStr for ElementSelector {
    type Err = String;
    fn from_str(text: &str) -> Result<Self, Self::Err> {
        if let Some(native) = text.strip_prefix("native:") {
            return native
                .parse()
                .map(|native_id| Self::Native { native_id })
                .map_err(|error: InvalidId| error.to_string());
        }
        text.parse().map(Self::Xml).map_err(str::to_owned)
    }
}
impl From<XmlId> for ElementIdentity {
    fn from(id: XmlId) -> Self {
        Self::Legacy(id)
    }
}
impl From<XmlId> for ElementSelector {
    fn from(id: XmlId) -> Self {
        Self::Xml(id)
    }
}
impl FromStr for ElementIdentity {
    type Err = &'static str;
    fn from_str(text: &str) -> Result<Self, Self::Err> {
        text.parse().map(Self::Legacy)
    }
}
impl fmt::Display for ElementIdentity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.selector().fmt(f)
    }
}
impl PartialEq<ElementSelector> for ElementIdentity {
    fn eq(&self, other: &ElementSelector) -> bool {
        self.selector() == *other
    }
}
impl PartialEq<ElementIdentity> for ElementSelector {
    fn eq(&self, other: &ElementIdentity) -> bool {
        *self == other.selector()
    }
}
impl PartialEq<XmlId> for ElementSelector {
    fn eq(&self, other: &XmlId) -> bool {
        matches!(self, Self::Xml(id) if id == other)
    }
}
impl PartialEq<ElementSelector> for XmlId {
    fn eq(&self, other: &ElementSelector) -> bool {
        other == self
    }
}
impl PartialEq<XmlId> for ElementIdentity {
    fn eq(&self, other: &XmlId) -> bool {
        matches!(self, Self::Legacy(id) if id == other)
    }
}
impl PartialEq<ElementIdentity> for XmlId {
    fn eq(&self, other: &ElementIdentity) -> bool {
        other == self
    }
}
impl fmt::Display for ElementSelector {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Xml(id) => id.fmt(f),
            Self::Native { native_id } => write!(f, "native:{native_id}"),
        }
    }
}
impl PartialEq for ElementIdentity {
    fn eq(&self, other: &Self) -> bool {
        self.selector() == other.selector()
    }
}
impl Eq for ElementIdentity {}
impl std::hash::Hash for ElementIdentity {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.selector().hash(state);
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ElementKey {
    pub document_id: DocumentId,
    pub xml_id: ElementSelector,
}
