//! Dictionary entry and folder mutations.
use super::*;
pub(super) fn rename_folder(
    p: &mut ProjectData,
    folder_id: XmlId,
    name: String,
) -> Result<(), CommandError> {
    p.folders
        .iter_mut()
        .find(|f| f.id == folder_id)
        .ok_or_else(|| invalid("folder not found"))?
        .name = name;
    Ok(())
}
pub(super) fn move_folder(
    p: &mut ProjectData,
    folder_id: XmlId,
    parent: Option<XmlId>,
) -> Result<(), CommandError> {
    p.folders
        .iter_mut()
        .find(|f| f.id == folder_id)
        .ok_or_else(|| invalid("folder not found"))?
        .parent = parent;
    Ok(())
}
pub(super) fn delete_folder(p: &mut ProjectData, folder_id: XmlId) -> Result<(), CommandError> {
    if p.folders
        .iter()
        .any(|f| f.parent.as_ref() == Some(&folder_id))
        || p.documents
            .iter()
            .any(|d| d.folder.as_ref() == Some(&folder_id))
    {
        return Err(invalid("folder is not empty"));
    }
    let len = p.folders.len();
    p.folders.retain(|f| f.id != folder_id);
    if len == p.folders.len() {
        return Err(invalid("folder not found"));
    }
    Ok(())
}
pub(super) fn update_dictionary_entry(
    p: &mut ProjectData,
    entry: DictionaryEntry,
) -> Result<(), CommandError> {
    let current = p
        .dictionary_entries
        .iter_mut()
        .find(|e| e.id == entry.id)
        .ok_or_else(|| invalid("entry not found"))?;
    if current.kind != entry.kind {
        return Err(invalid("entry kind cannot change"));
    }
    *current = entry;
    Ok(())
}
pub(super) fn delete_dictionary_entry(
    p: &mut ProjectData,
    entry_id: DictionaryEntryId,
    resolution: DictionaryDeleteResolution,
) -> Result<(), CommandError> {
    let entry = p
        .dictionary_entries
        .iter()
        .find(|e| e.id == entry_id)
        .ok_or_else(|| invalid("entry not found"))?
        .clone();
    let used = p
        .dictionary_links
        .iter()
        .any(|l| l.target == DictionaryTarget::Entry(entry_id))
        || crate::attributes::dictionary_entry_in_use(p, &DictionaryTarget::Entry(entry_id));
    if used {
        let replacement = match resolution {
            DictionaryDeleteResolution::Reject => return Err(invalid("entry is in use")),
            DictionaryDeleteResolution::Unlink => DictionaryTarget::Literal(entry.name),
            DictionaryDeleteResolution::Replace(id) => {
                if id == entry_id
                    || !p
                        .dictionary_entries
                        .iter()
                        .any(|e| e.id == id && e.kind == entry.kind)
                {
                    return Err(invalid("invalid replacement"));
                }
                DictionaryTarget::Entry(id)
            }
        };
        for l in &mut p.dictionary_links {
            if l.target == DictionaryTarget::Entry(entry_id) {
                l.target = replacement.clone();
            }
        }
        let mut seen = std::collections::HashSet::new();
        p.dictionary_links
            .retain(|l| seen.insert((l.source.clone(), l.kind, l.target.clone())));
        crate::attributes::retarget_dictionary_refs(
            p,
            &DictionaryTarget::Entry(entry_id),
            &replacement,
        );
    }
    p.dictionary_entries.retain(|e| e.id != entry_id);
    Ok(())
}
pub(super) fn set_dictionary_links(
    p: &mut ProjectData,
    source: ElementKey,
    kind: DictionaryKind,
    targets: Vec<DictionaryTarget>,
) -> Result<(), CommandError> {
    doc_structural(p, source.document_id)?;
    p.dictionary_links
        .retain(|l| l.source != source || l.kind != kind);
    p.dictionary_links
        .extend(targets.into_iter().map(|target| DictionaryLink {
            source: source.clone(),
            kind,
            target,
        }));
    Ok(())
}
