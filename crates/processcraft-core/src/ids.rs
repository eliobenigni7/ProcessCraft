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
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, schemars::JsonSchema)]
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
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ElementKey {
    pub document_id: DocumentId,
    pub xml_id: XmlId,
}
