//! Reorder rootElement fragments recorded directly by the typed XML writer.
//! Ranges and identities are transient; no XML marker or identifier is invented.
use processcraft_core::ids::{ElementIdentity, ElementSelector};
use std::collections::{HashMap, HashSet};
use std::ops::Range;

pub(crate) struct RootSpans {
    enabled: bool,
    spans: Vec<(ElementSelector, Range<usize>)>,
    identities: HashSet<ElementSelector>,
}

impl RootSpans {
    pub(crate) fn new(enabled: bool) -> Self {
        Self {
            enabled,
            spans: Vec::new(),
            identities: HashSet::new(),
        }
    }

    pub(crate) fn record(
        &mut self,
        identity: Option<&ElementIdentity>,
        start: usize,
        end: usize,
    ) -> Result<(), String> {
        if !self.enabled {
            return Ok(());
        }
        let identity = identity.ok_or(
            "Initialize root order by explicitly assigning a native identity to each anonymous root",
        )?;
        if start >= end
            || self
                .spans
                .last()
                .is_some_and(|(_, range)| range.end > start)
        {
            return Err("Root writer recorded overlapping or empty fragments".into());
        }
        let selector = identity.selector();
        if !self.identities.insert(selector.clone()) {
            return Err("Root order contains duplicate root identities".into());
        }
        self.spans.push((selector, start..end));
        Ok(())
    }

    pub(crate) fn finish(
        &self,
        xml: &mut String,
        order: Option<&[ElementSelector]>,
    ) -> Result<(), String> {
        let Some(order) = order else {
            return if self.enabled {
                Err("Root order was removed while writing the document".into())
            } else {
                Ok(())
            };
        };
        if !self.enabled || order.len() != self.spans.len() {
            return Err("Root order must include every written root exactly once".into());
        }
        let mut requested = HashSet::with_capacity(order.len());
        if order
            .iter()
            .any(|id| !self.identities.contains(id) || !requested.insert(id))
        {
            return Err("Root order must include every written root exactly once".into());
        }
        for (index, (_, range)) in self.spans.iter().enumerate() {
            if xml.get(range.clone()).is_none() {
                return Err("Root writer recorded an invalid XML fragment boundary".into());
            }
            if let Some((_, next)) = self.spans.get(index + 1)
                && !xml[range.end..next.start].chars().all(char::is_whitespace)
            {
                return Err("An unrecorded XML element separates ordered roots".into());
            }
        }
        let Some((_, first)) = self.spans.first() else {
            return Ok(());
        };
        let by_identity: HashMap<_, _> = self.spans.iter().map(|(id, range)| (id, range)).collect();
        let mut reordered = String::with_capacity(xml.len());
        reordered.push_str(&xml[..first.start]);
        for (index, id) in order.iter().enumerate() {
            reordered.push_str(&xml[by_identity[id].clone()]);
            let original = &self.spans[index].1;
            if let Some((_, next)) = self.spans.get(index + 1) {
                reordered.push_str(&xml[original.end..next.start]);
            } else {
                reordered.push_str(&xml[original.end..]);
            }
        }
        *xml = reordered;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use processcraft_core::ids::XmlId;

    fn identity(value: &str) -> ElementIdentity {
        value.parse::<XmlId>().unwrap().into()
    }

    #[test]
    fn roots_reorder_without_touching_payload_imports_or_diagrams() {
        let a = identity("A");
        let b = identity("B");
        let prefix = "<definitions><import/>";
        let first = "<process><extension>é&lt;opaque&gt;</extension></process>";
        let second = "<message/>";
        let suffix = "<BPMNDiagram/><relationship/></definitions>";
        let mut xml = format!("{prefix}{first}\n {second}{suffix}");
        let mut spans = RootSpans::new(true);
        spans
            .record(Some(&a), prefix.len(), prefix.len() + first.len())
            .unwrap();
        let start = prefix.len() + first.len() + 2;
        spans.record(Some(&b), start, start + second.len()).unwrap();
        spans
            .finish(&mut xml, Some(&[b.selector(), a.selector()]))
            .unwrap();
        assert_eq!(xml, format!("{prefix}{second}\n {first}{suffix}"));
    }

    #[test]
    fn native_only_roots_need_no_xml_ids_and_legacy_mode_is_untouched() {
        let a = ElementIdentity::anonymous();
        let mut xml = "<definitions><message/></definitions>".to_owned();
        let mut spans = RootSpans::new(true);
        spans.record(Some(&a), 13, 23).unwrap();
        spans.finish(&mut xml, Some(&[a.selector()])).unwrap();
        assert!(!xml.contains("id="));
        let original = xml.clone();
        let mut legacy = RootSpans::new(false);
        legacy.record(None, 0, 0).unwrap();
        legacy.finish(&mut xml, None).unwrap();
        assert_eq!(xml, original);
    }

    #[test]
    fn invalid_order_or_hidden_fragments_fail_without_changing_xml() {
        let a = identity("A");
        let b = identity("B");
        let mut spans = RootSpans::new(true);
        spans.record(Some(&a), 0, 4).unwrap();
        spans.record(Some(&b), 8, 12).unwrap();
        let mut xml = "<a/><x/><b/>".to_owned();
        let original = xml.clone();
        assert!(
            spans
                .finish(&mut xml, Some(&[a.selector(), a.selector()]))
                .is_err()
        );
        assert_eq!(xml, original);
        assert!(
            spans
                .finish(&mut xml, Some(&[b.selector(), a.selector()]))
                .is_err()
        );
        assert_eq!(xml, original);
        assert!(spans.record(None, 12, 15).is_err());
    }
}
