//! Checkpoint and comment mutations.
use super::*;
pub(super) fn create_checkpoint(
    p: &mut ProjectData,
    id: CheckpointId,
    name: String,
    created_unix_ms: u64,
) -> Result<(), CommandError> {
    if p.checkpoints.len() >= crate::MAX_CHECKPOINTS {
        return Err(invalid("checkpoint limit exceeded"));
    }
    if p.checkpoints.iter().any(|c| c.id == id) {
        return Err(invalid("duplicate checkpoint ID"));
    }
    p.checkpoints.push(Checkpoint {
        id,
        name,
        created_unix_ms,
        content: FrozenProject::capture(p),
    });
    Ok(())
}
pub(super) fn rename_checkpoint(
    p: &mut ProjectData,
    id: CheckpointId,
    name: String,
) -> Result<(), CommandError> {
    p.checkpoints
        .iter_mut()
        .find(|c| c.id == id)
        .ok_or_else(|| invalid("checkpoint not found"))?
        .name = name;
    Ok(())
}
pub(super) fn delete_checkpoint(p: &mut ProjectData, id: CheckpointId) -> Result<(), CommandError> {
    if p.reference_checkpoint == Some(id) {
        return Err(invalid(
            "clear the reference before deleting this checkpoint",
        ));
    }
    let index = p
        .checkpoints
        .iter()
        .position(|c| c.id == id)
        .ok_or_else(|| invalid("checkpoint not found"))?;
    p.checkpoints.remove(index);
    Ok(())
}
pub(super) fn restore_checkpoint(
    p: &mut ProjectData,
    id: CheckpointId,
) -> Result<(), CommandError> {
    let checkpoint = p
        .checkpoints
        .iter()
        .find(|c| c.id == id)
        .ok_or_else(|| invalid("checkpoint not found"))?;
    let mut restored = checkpoint.content.project();
    restored.checkpoints = std::mem::take(&mut p.checkpoints);
    restored.reference_checkpoint = p.reference_checkpoint;
    *p = restored;
    Ok(())
}
pub(super) fn create_comment(p: &mut ProjectData, comment: Comment) -> Result<(), CommandError> {
    if !selection_exists(p, &comment.target) {
        return Err(invalid("comment target not found"));
    }
    if p.comments.iter().any(|c| c.id == comment.id) {
        return Err(invalid("duplicate comment ID"));
    }
    p.comments.push(comment);
    Ok(())
}
pub(super) fn update_comment(
    p: &mut ProjectData,
    comment_id: CommentId,
    author: String,
    text: String,
) -> Result<(), CommandError> {
    let comment = p
        .comments
        .iter_mut()
        .find(|c| c.id == comment_id)
        .ok_or_else(|| invalid("comment not found"))?;
    comment.author = author;
    comment.text = text;
    Ok(())
}
pub(super) fn delete_comment(
    p: &mut ProjectData,
    comment_id: CommentId,
) -> Result<(), CommandError> {
    let i = p
        .comments
        .iter()
        .position(|c| c.id == comment_id)
        .ok_or_else(|| invalid("comment not found"))?;
    p.comments.remove(i);
    Ok(())
}
