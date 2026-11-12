use super::*;
use processcraft_core::{
    ids::{CheckpointId, CommentId, ElementKey},
    layout::{LayoutProposal, propose_layout},
};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc,
};

#[derive(Default)]
pub(super) struct State {
    session: String,
    comments_open: bool,
    checkpoints_open: bool,
    target: Option<ElementKey>,
    author: String,
    text: String,
    comments_page: usize,
    editing: Option<CommentId>,
    checkpoint: Option<CheckpointId>,
    checkpoint_name: String,
    comparison: Option<(
        u64,
        CheckpointId,
        processcraft_core::queries::CheckpointComparison,
    )>,
    restore: Option<CheckpointId>,
    reference_view: Option<(CheckpointId, usize)>,
    reference_page: usize,
    comparison_page: usize,
    reference_diagram: Option<ElementSelector>,
    reference_key: String,
    reference_error: String,
    reference_image: Option<egui::TextureHandle>,
    reference_worker: Option<mpsc::Receiver<ReferenceImage>>,
    worker: Option<mpsc::Receiver<Result<LayoutProposal, String>>>,
    cancel: Option<Arc<AtomicBool>>,
    pub(super) preview: Option<(LayoutProposal, Arc<BpmnDocument>)>,
}
const COMPARISON_PAGE_SIZE: usize = 100;
const PREVIEW_LIMIT: usize = 500;

/// Serialize only a bounded prefix. Large values are fully serialized only on an explicit copy.
fn value_preview(value: &serde_json::Value) -> (String, bool) {
    if value.is_null() {
        return ("Unset".into(), false);
    }
    if let Some(text) = value.as_str() {
        let mut chars = text.chars();
        let preview = chars.by_ref().take(PREVIEW_LIMIT).collect();
        return (preview, chars.next().is_some());
    }
    struct Prefix {
        bytes: Vec<u8>,
        truncated: bool,
    }
    impl std::io::Write for Prefix {
        fn write(&mut self, data: &[u8]) -> std::io::Result<usize> {
            let remaining = PREVIEW_LIMIT.saturating_sub(self.bytes.len());
            if data.len() > remaining {
                self.bytes.extend_from_slice(&data[..remaining]);
                self.truncated = true;
                return Err(std::io::Error::other("preview complete"));
            }
            self.bytes.extend_from_slice(data);
            Ok(data.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut prefix = Prefix {
        bytes: Vec::with_capacity(PREVIEW_LIMIT),
        truncated: false,
    };
    let _ = serde_json::to_writer(&mut prefix, value);
    (
        String::from_utf8_lossy(&prefix.bytes).into_owned(),
        prefix.truncated,
    )
}
fn copy_value(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::Null => "Unset".into(),
        serde_json::Value::String(text) => text.clone(),
        value => value.to_string(),
    }
}
fn property_detail_ui(
    ui: &mut egui::Ui,
    property: &processcraft_core::queries::PropertyDifference,
) {
    let (before, before_short) = value_preview(&property.before);
    let (after, after_short) = value_preview(&property.after);
    let label = property
        .property
        .trim_matches('/')
        .replace(['_', '.', '/'], " ");
    ui.label(
        egui::RichText::new(if label.is_empty() {
            "Whole value"
        } else {
            &label
        })
        .strong(),
    );
    ui.label(format!("Before: {before}"));
    ui.label(format!("After: {after}"));
    if before_short || after_short {
        ui.label("Preview shortened to 500 characters. Copy a value to inspect it in full.");
    }
    ui.horizontal(|ui| {
        if ui.small_button("Copy previous value").clicked() {
            ui.ctx().copy_text(copy_value(&property.before));
        }
        if ui.small_button("Copy current value").clicked() {
            ui.ctx().copy_text(copy_value(&property.after));
        }
    });
}
fn semantic_property_page(
    changes: &[processcraft_core::queries::ElementChange],
    page: usize,
) -> impl Iterator<
    Item = (
        usize,
        &processcraft_core::queries::ElementChange,
        &processcraft_core::queries::PropertyDifference,
    ),
> {
    changes
        .iter()
        .flat_map(|change| {
            change
                .properties
                .iter()
                .map(move |property| (change, property))
        })
        .enumerate()
        .skip(page.saturating_mul(COMPARISON_PAGE_SIZE))
        .take(COMPARISON_PAGE_SIZE)
        .map(|(index, (change, property))| (index, change, property))
}
fn diagram_property_page(
    changes: &[processcraft_core::queries::DiagramChange],
    page: usize,
) -> impl Iterator<
    Item = (
        usize,
        &processcraft_core::queries::DiagramChange,
        &processcraft_core::queries::PropertyDifference,
    ),
> {
    changes
        .iter()
        .flat_map(|change| {
            change
                .properties
                .iter()
                .map(move |property| (change, property))
        })
        .enumerate()
        .skip(page.saturating_mul(COMPARISON_PAGE_SIZE))
        .take(COMPARISON_PAGE_SIZE)
        .map(|(index, (change, property))| (index, change, property))
}
fn diagram_label(document: &BpmnDocument, selector: &ElementSelector) -> String {
    document
        .diagrams
        .iter()
        .enumerate()
        .find(|(_, diagram)| diagram.id == *selector)
        .map_or_else(
            || "Unavailable diagram".into(),
            |(i, diagram)| {
                diagram
                    .name
                    .as_deref()
                    .filter(|name| !name.is_empty())
                    .map(str::to_owned)
                    .unwrap_or_else(|| format!("Diagram {}", i + 1))
            },
        )
}
fn diagram_representation_label(
    document: &BpmnDocument,
    diagram_id: &ElementSelector,
    representation_id: &ElementSelector,
) -> String {
    let Some(diagram) = document
        .diagrams
        .iter()
        .find(|diagram| diagram.id == *diagram_id)
    else {
        return "Unavailable representation".into();
    };
    if let Some((i, shape)) = diagram
        .shapes
        .iter()
        .enumerate()
        .find(|(_, shape)| shape.id == *representation_id)
    {
        return shape.id.xml_id().map_or_else(
            || format!("Shape {}", i + 1),
            |id| format!("Shape {} · {id}", i + 1),
        );
    }
    if let Some((i, edge)) = diagram
        .edges
        .iter()
        .enumerate()
        .find(|(_, edge)| edge.id == *representation_id)
    {
        return edge.id.xml_id().map_or_else(
            || format!("Edge {}", i + 1),
            |id| format!("Edge {} · {id}", i + 1),
        );
    }
    "Unavailable representation".into()
}
struct ReferenceImage {
    key: String,
    pixels: Result<(Vec<u8>, usize, usize), String>,
}
impl State {
    pub(super) fn has_draft(&self) -> bool {
        !self.text.is_empty() || self.editing.is_some()
    }
}
impl Drop for State {
    fn drop(&mut self) {
        if let Some(cancel) = &self.cancel {
            cancel.store(true, Ordering::Relaxed);
        }
    }
}

impl ProcessCraft {
    pub(super) fn m3_poll(&mut self, ctx: &egui::Context) {
        let session = self.ctl.session.id();
        if self.m3.session != session {
            self.m3 = State::default();
            self.m3.session = session;
        }
        if let Some(rx) = &self.m3.worker {
            match rx.try_recv() {
                Ok(result) => {
                    let cancelled = self
                        .m3
                        .cancel
                        .as_ref()
                        .is_some_and(|c| c.load(Ordering::Relaxed));
                    self.m3.worker = None;
                    self.m3.cancel = None;
                    if cancelled {
                        return;
                    }
                    match result {
                        Ok(proposal) => {
                            let snap = self.ctl.session.snapshot();
                            if proposal.session_id != self.ctl.session.id()
                                || proposal.generation != snap.generation
                            {
                                self.ctl.error="Auto-layout result is stale. Preview again on the current model.".into();
                            } else if let Some(doc) = snap
                                .project
                                .documents
                                .iter()
                                .find(|d| d.id == proposal.document_id)
                            {
                                let mut preview = (**doc).clone();
                                if let Some(diagram) = preview
                                    .diagrams
                                    .iter_mut()
                                    .find(|d| d.id == proposal.diagram_id)
                                {
                                    for shape in &mut diagram.shapes {
                                        if let Some(next) = proposal
                                            .proposed
                                            .shapes
                                            .iter()
                                            .find(|s| s.id == shape.id)
                                        {
                                            shape.bounds = next.bounds.clone();
                                            if let (Some(label), Some(bounds)) =
                                                (&mut shape.label, &next.label_bounds)
                                            {
                                                label.bounds = Some(bounds.clone());
                                            }
                                        }
                                    }
                                    for edge in &mut diagram.edges {
                                        if let Some(next) =
                                            proposal.proposed.edges.iter().find(|e| e.id == edge.id)
                                        {
                                            edge.waypoints = next.waypoints.clone();
                                            if let (Some(label), Some(bounds)) =
                                                (&mut edge.label, &next.label_bounds)
                                            {
                                                label.bounds = Some(bounds.clone());
                                            }
                                        }
                                    }
                                    self.m3.preview = Some((proposal, Arc::new(preview)));
                                }
                            }
                        }
                        Err(error) => self.ctl.error = error,
                    }
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.m3.worker = None;
                    self.m3.cancel = None;
                    self.ctl.error = "Auto-layout worker stopped. No layout was applied.".into();
                }
                Err(mpsc::TryRecvError::Empty) => {
                    ctx.request_repaint_after(std::time::Duration::from_millis(30))
                }
            }
        }
        let snap = self.ctl.session.snapshot();
        if self.m3.preview.as_ref().is_some_and(|(p, _)| {
            p.session_id != self.ctl.session.id() || p.generation != snap.generation
        }) {
            self.m3.preview = None;
            self.ctl.error =
                "Auto-layout preview was discarded because the project changed.".into();
        }
    }
    pub(super) fn layout_preview_document(&self) -> Option<Arc<BpmnDocument>> {
        let snap = self.ctl.session.snapshot();
        self.m3
            .preview
            .as_ref()
            .filter(|(p, _)| {
                p.session_id == self.ctl.session.id()
                    && p.generation == snap.generation
                    && snap
                        .project
                        .documents
                        .get(self.doc)
                        .is_some_and(|d| d.id == p.document_id)
                    && self
                        .diagram_id
                        .as_ref()
                        .is_none_or(|id| id == &p.diagram_id)
            })
            .map(|(_, doc)| doc.clone())
    }
    pub(super) fn start_layout(&mut self, ctx: &egui::Context) {
        if self.m3.worker.is_some() || !self.prepare_navigation() {
            return;
        }
        let snap = self.ctl.session.snapshot();
        let Some(doc) = snap.project.documents.get(self.doc).cloned() else {
            return;
        };
        let Some(diagram) = doc
            .diagrams
            .iter()
            .find(|d| self.diagram_id.as_ref().is_none_or(|id| id == &d.id))
            .map(|d| d.id.selector())
        else {
            self.ctl.error = "This model has no diagram to arrange.".into();
            return;
        };
        let session = self.ctl.session.id();
        let cancel = Arc::new(AtomicBool::new(false));
        let work_cancel = cancel.clone();
        let ctx = ctx.clone();
        let (tx, rx) = mpsc::channel();
        self.m3.preview = None;
        self.m3.worker = Some(rx);
        self.m3.cancel = Some(cancel);
        std::thread::spawn(move || {
            let result = propose_layout(&doc, &diagram, &session, snap.generation, &work_cancel)
                .map_err(|e| e.to_string());
            let _ = tx.send(result);
            ctx.request_repaint();
        });
    }
    pub(super) fn layout_bar(&mut self, ui: &mut egui::Ui) {
        if self.m3.worker.is_some() {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label("Preparing auto-layout preview…");
                if ui.button("Cancel layout").clicked()
                    && let Some(c) = &self.m3.cancel
                {
                    c.store(true, Ordering::Relaxed);
                }
            });
        }
        if let Some((proposal, _)) = self.m3.preview.clone() {
            let snap = self.ctl.session.snapshot();
            let active = snap.project.documents.get(self.doc).is_some_and(|d| {
                d.id == proposal.document_id
                    && d.diagrams
                        .iter()
                        .find(|v| self.diagram_id.as_ref().is_none_or(|id| &v.id == id))
                        .is_some_and(|v| v.id == proposal.diagram_id)
            });
            ui.horizontal_wrapped(|ui| {
                ui.strong("Auto-layout preview");
                ui.label(format!(
                    "Diagram {} · project unchanged until Apply",
                    proposal.diagram_id
                ));
                if ui
                    .add_enabled(
                        active && !self.has_drafts(),
                        egui::Button::new("Apply layout"),
                    )
                    .clicked()
                    && self.ctl.mutate(vec![Mutation::ApplyLayout { proposal }])
                {
                    self.m3.preview = None;
                }
                if ui.button("Cancel preview").clicked() {
                    self.m3.preview = None;
                }
            });
        }
    }
    pub(super) fn open_comments(&mut self) {
        let snap = self.ctl.session.snapshot();
        if !self.m3.has_draft() {
            self.m3.target = self.selected.as_ref().and_then(|xml_id| {
                snap.project.documents.get(self.doc).map(|d| ElementKey {
                    document_id: d.id,
                    xml_id: xml_id.clone(),
                })
            });
        }
        self.m3.comments_open = true;
    }
    pub(super) fn open_checkpoints(&mut self) {
        self.m3.checkpoints_open = true;
    }
    pub(super) fn m3_windows(&mut self, ctx: &egui::Context) {
        if self.m3.comments_open {
            let mut open = true;
            egui::Window::new("Local comments")
                .open(&mut open)
                .default_width(560.0)
                .max_width((ctx.content_rect().width() - 32.0).max(150.0))
                .default_height(500.0)
                .max_height((ctx.content_rect().height() - 64.0).max(100.0))
                .vscroll(true)
                .show(ctx, |ui| self.comments_ui(ui));
            self.m3.comments_open = open;
        }
        if self.m3.checkpoints_open {
            let mut open = true;
            egui::Window::new("Checkpoints and local reference")
                .open(&mut open)
                .default_width(600.0)
                .max_width((ctx.content_rect().width() - 32.0).max(150.0))
                .default_height(520.0)
                .max_height((ctx.content_rect().height() - 64.0).max(100.0))
                .vscroll(true)
                .show(ctx, |ui| self.checkpoints_ui(ui));
            self.m3.checkpoints_open = open;
        }
        self.reference_window(ctx);
        if let Some(id) = self.m3.restore {
            egui::Window::new("Restore checkpoint?").collapsible(false).show(ctx,|ui| {
                ui.label("This replaces the current project content with the selected checkpoint, including dictionary and comments. The checkpoint catalog stays available. Restore is one undoable change.");
                ui.horizontal(|ui| {
                    if ui.button("Cancel restore").clicked() {self.m3.restore=None;}
                    if ui.button("Restore project content").clicked() && self.prepare_navigation()
                        && self.ctl.mutate(vec![Mutation::RestoreCheckpoint{id}]) {
                        self.doc=self.doc.min(self.ctl.session.snapshot().project.documents.len().saturating_sub(1));
                        self.selected=None;self.multi.clear();self.process_id=None;self.diagram_id=None;self.m3.preview=None;self.m3.target=None;self.m3.editing=None;self.select(None);self.refresh_dictionary_editor();self.m3.restore=None;
                    }
                });
            });
        }
    }
    fn comments_ui(&mut self, ui: &mut egui::Ui) {
        let snap = self.ctl.session.snapshot();
        ui.label("Plain-text local notes. Author names are descriptive and are not authenticated.");
        let pages = snap.project.comments.len().div_ceil(50).max(1);
        self.m3.comments_page = self.m3.comments_page.min(pages - 1);
        ui.horizontal(|ui| {
            if ui
                .add_enabled(
                    self.m3.comments_page > 0,
                    egui::Button::new("Previous comments"),
                )
                .clicked()
            {
                self.m3.comments_page -= 1;
            }
            ui.label(format!(
                "Page {} of {} · {} comments",
                self.m3.comments_page + 1,
                pages,
                snap.project.comments.len()
            ));
            if ui
                .add_enabled(
                    self.m3.comments_page + 1 < pages,
                    egui::Button::new("Next comments"),
                )
                .clicked()
            {
                self.m3.comments_page += 1;
            }
        });
        for comment in snap
            .project
            .comments
            .iter()
            .skip(self.m3.comments_page * 50)
            .take(50)
        {
            ui.push_id(comment.id.to_string(), |ui| {
                ui.separator();
                ui.horizontal_wrapped(|ui| {
                    ui.strong(if comment.author.is_empty() {
                        "Anonymous"
                    } else {
                        &comment.author
                    });
                    ui.label(format!(
                        "{} / {}",
                        comment.target.document_id, comment.target.xml_id
                    ));
                    if processcraft_core::queries::comment_is_orphan(&snap.project, comment) {
                        ui.colored_label(self.theme.palette().warning, "Orphaned target");
                    }
                });
                ui.label(&comment.text);
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(!self.m3.has_draft(), egui::Button::new("Edit comment"))
                        .clicked()
                    {
                        self.m3.target = Some(comment.target.clone());
                        self.m3.author = comment.author.clone();
                        self.m3.text = comment.text.clone();
                        self.m3.editing = Some(comment.id);
                    }
                    if ui
                        .add_enabled(!self.m3.has_draft(), egui::Button::new("Delete comment"))
                        .clicked()
                    {
                        self.ctl.mutate(vec![Mutation::DeleteComment {
                            comment_id: comment.id,
                        }]);
                    }
                });
            });
        }
        ui.separator();
        if let Some(target) = self.m3.target.clone() {
            ui.strong(if self.m3.editing.is_some() {
                "Edit local comment"
            } else {
                "New comment"
            });
            ui.label(format!(
                "Target: {} / {}",
                target.document_id, target.xml_id
            ));
            if self.m3.editing.is_none()
                && ui.button("Attach draft to current selection").clicked()
                && let Some(xml_id) = &self.selected
                && let Some(doc) = snap.project.documents.get(self.doc)
            {
                self.m3.target = Some(ElementKey {
                    document_id: doc.id,
                    xml_id: xml_id.clone(),
                });
            }
            let label = ui.label("Author");
            ui.text_edit_singleline(&mut self.m3.author)
                .labelled_by(label.id);
            let label = ui.label("Comment");
            ui.add(
                egui::TextEdit::multiline(&mut self.m3.text)
                    .desired_rows(4)
                    .desired_width(f32::INFINITY),
            )
            .labelled_by(label.id);
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(
                        !self.m3.text.trim().is_empty(),
                        egui::Button::new("Post comment"),
                    )
                    .clicked()
                {
                    let mutation = if let Some(comment_id) = self.m3.editing {
                        Mutation::UpdateComment {
                            comment_id,
                            author: self.m3.author.clone(),
                            text: self.m3.text.clone(),
                        }
                    } else {
                        Mutation::CreateComment {
                            comment: Comment {
                                id: CommentId::new(),
                                target,
                                author: self.m3.author.clone(),
                                text: self.m3.text.clone(),
                            },
                        }
                    };
                    if self.ctl.mutate(vec![mutation]) {
                        self.m3.text.clear();
                        self.m3.editing = None;
                    }
                }
                if ui.button("Discard comment draft").clicked() {
                    self.m3.text.clear();
                    self.m3.editing = None;
                }
            });
        } else {
            ui.label("Select an element in the model, then open Comments to add a note.");
        }
    }
    fn reference_window(&mut self, ctx: &egui::Context) {
        let Some((id, mut index)) = self.m3.reference_view else {
            return;
        };
        let snapshot = self.ctl.session.snapshot();
        let Some(checkpoint) = snapshot.project.checkpoints.iter().find(|c| c.id == id) else {
            self.m3.reference_view = None;
            return;
        };
        let mut open = true;
        egui::Window::new(format!("Frozen checkpoint · {}", checkpoint.name)).open(&mut open).default_width(760.0).max_width((ctx.content_rect().width()-32.0).max(150.0)).max_height((ctx.content_rect().height()-64.0).max(100.0)).vscroll(true).show(ctx, |ui| {
            ui.label("Read-only snapshot. Names, dictionary values and comments come from this checkpoint.");
            egui::ComboBox::from_label("Frozen model").selected_text(checkpoint.content.documents.get(index).map_or("No models", |d| d.name.as_str())).show_ui(ui, |ui| {
                for (i, doc) in checkpoint.content.documents.iter().enumerate() { ui.selectable_value(&mut index, i, &doc.name); }
            });
            let node_count = checkpoint.content.documents.get(index).map_or(0,|d|processcraft_core::queries::scopes(d).iter().map(|s|s.nodes.len()).sum::<usize>());
            let pages = node_count.max(checkpoint.content.dictionary_entries.len()).max(checkpoint.content.comments.len()).div_ceil(COMPARISON_PAGE_SIZE).max(1);
            self.m3.reference_page = self.m3.reference_page.min(pages-1);
            ui.horizontal(|ui| {
                if ui.add_enabled(self.m3.reference_page > 0,egui::Button::new("Previous frozen details")).clicked() {self.m3.reference_page-=1;}
                ui.label(format!("Details page {} of {} · 100 entries per section",self.m3.reference_page+1,pages));
                if ui.add_enabled(self.m3.reference_page+1 < pages,egui::Button::new("Next frozen details")).clicked() {self.m3.reference_page+=1;}
            });
            if let Some(document) = checkpoint.content.documents.get(index) {
                egui::ComboBox::from_label("Frozen diagram").selected_text(self.m3.reference_diagram.as_ref().map_or("First diagram".into(), ToString::to_string)).show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.m3.reference_diagram, None, "First diagram");
                    for diagram in &document.diagrams { ui.selectable_value(&mut self.m3.reference_diagram, Some(diagram.id.selector()), diagram.id.to_string()); }
                });
                if let Some(diagram) = document.diagrams.iter().find(|d| self.m3.reference_diagram.as_ref().is_none_or(|id| &d.id == id)).or_else(|| document.diagrams.first()) {
                    let key = format!("{id}:{index}:{}:{}", diagram.id, self.theme == Theme::Dark);
                    if self.m3.reference_key != key && self.m3.reference_worker.is_none() {
                        self.m3.reference_key = key.clone(); self.m3.reference_image = None; self.m3.reference_error.clear();
                        let (tx, rx) = mpsc::sync_channel(1); self.m3.reference_worker = Some(rx);
                        let document = document.clone(); let diagram = diagram.id.selector(); let appearance = if self.theme == Theme::Dark { Appearance::Dark } else { Appearance::Light }; let wake = ctx.clone();
                        let worker_key = key.clone();
                        std::thread::spawn(move || {
                            let selected = document.diagrams.iter().find(|d| d.id == diagram).unwrap();
                            let (x, y, right, bottom) = diagram_extents(selected).unwrap_or((0.0,0.0,500.0,300.0));
                            let zoom = (1024.0/(right-x+40.0)).min(600.0/(bottom-y+40.0)).clamp(0.125,8.0) as f32;
                            let view = processcraft_render::Viewport { x:x-20.0,y:y-20.0,width:1024.0/zoom as f64,height:600.0/zoom as f64,zoom };
                            let pixels = processcraft_render::BpmnViewport::build(&document,&diagram,appearance,&mut processcraft_render::TextEngine::new(),"Noto Sans",view).map(|r|(r.pixels.data().to_vec(),r.pixels.width() as usize,r.pixels.height() as usize)).map_err(|e|e.to_string());
                            let _ = tx.send(ReferenceImage { key: worker_key,pixels }); wake.request_repaint();
                        });
                    }
                    if let Some(rx) = &self.m3.reference_worker && let Ok(result) = rx.try_recv() {
                        self.m3.reference_worker = None;
                        if result.key == key { match result.pixels { Ok((pixels,w,h)) => self.m3.reference_image = Some(ctx.load_texture("frozen-checkpoint",egui::ColorImage::from_rgba_unmultiplied([w,h],&pixels),Default::default())), Err(error) => { self.m3.reference_error = error; } } }
                    }
                    if self.m3.reference_key == key && !self.m3.reference_error.is_empty() {
                        ui.label(&self.m3.reference_error);
                    } else if self.m3.reference_key == key && let Some(texture) = &self.m3.reference_image { ui.add(egui::Image::new(texture).max_width(ui.available_width()).maintain_aspect_ratio(true)); } else { ui.spinner(); ctx.request_repaint_after(std::time::Duration::from_millis(50)); }
                }
                ui.collapsing("Frozen element properties", |ui| {
                    for (scope, node) in processcraft_core::queries::scopes(document).into_iter().flat_map(|scope| scope.nodes.iter().map(move |node| (scope.name, node))).skip(self.m3.reference_page*100).take(100) { ui.label(format!("{} / {} · {} · {}",scope,node_kind_label(node.kind),node.name.as_deref().unwrap_or(""),node.documentation.plain_text().unwrap_or("Structured documentation"))); }
                });
            }
            ui.collapsing(format!("Frozen dictionary · {} entries",checkpoint.content.dictionary_entries.len()), |ui| { for entry in checkpoint.content.dictionary_entries.iter().skip(self.m3.reference_page*100).take(100) { ui.label(format!("{:?} · {} · {}",entry.kind,entry.name,entry.description)); } });
            ui.collapsing(format!("Frozen comments · {}",checkpoint.content.comments.len()), |ui| {
                for comment in checkpoint.content.comments.iter().skip(self.m3.reference_page*100).take(100) { ui.strong(&comment.author); ui.label(format!("{} / {}",comment.target.document_id,comment.target.xml_id)); ui.label(&comment.text); ui.separator(); }
            });
            ui.label(format!("{} linked resources",checkpoint.content.resources.len()));
        });
        self.m3.reference_view = open.then_some((id, index));
    }
    fn checkpoints_ui(&mut self, ui: &mut egui::Ui) {
        let snap = self.ctl.session.snapshot();
        ui.label(
            "Named local snapshots. A local reference is a comparison marker, not an approval.",
        );
        let label = ui.label("Checkpoint name");
        ui.text_edit_singleline(&mut self.m3.checkpoint_name)
            .labelled_by(label.id);
        ui.horizontal_wrapped(|ui| {
            if ui
                .add_enabled(
                    !self.m3.checkpoint_name.trim().is_empty()
                        && snap.project.checkpoints.len() < 20,
                    egui::Button::new("Create checkpoint"),
                )
                .clicked()
                && self.prepare_navigation()
            {
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis()
                    .min(u64::MAX as u128) as u64;
                let id = CheckpointId::new();
                if self.ctl.mutate(vec![Mutation::CreateCheckpoint {
                    id,
                    name: self.m3.checkpoint_name.clone(),
                    created_unix_ms: now,
                }]) {
                    self.m3.checkpoint = Some(id);
                }
            }
            if ui
                .add_enabled(
                    snap.project.reference_checkpoint.is_some(),
                    egui::Button::new("Clear local reference"),
                )
                .clicked()
            {
                self.ctl
                    .mutate(vec![Mutation::SetReferenceCheckpoint { id: None }]);
            }
        });
        for checkpoint in &snap.project.checkpoints {
            ui.horizontal_wrapped(|ui| {
                if ui
                    .selectable_label(self.m3.checkpoint == Some(checkpoint.id), &checkpoint.name)
                    .clicked()
                {
                    self.m3.checkpoint = Some(checkpoint.id);
                    self.m3.checkpoint_name = checkpoint.name.clone();
                }
                if snap.project.reference_checkpoint == Some(checkpoint.id) {
                    ui.label("Local reference");
                }
            });
        }
        if let Some(id) = self
            .m3
            .checkpoint
            .filter(|id| snap.project.checkpoints.iter().any(|c| c.id == *id))
        {
            ui.separator();
            if self
                .m3
                .comparison
                .as_ref()
                .is_none_or(|(generation, selected, _)| {
                    *generation != snap.generation || *selected != id
                })
            {
                self.m3.comparison =
                    processcraft_core::queries::compare_checkpoint(&snap.project, id)
                        .map(|diff| (snap.generation, id, diff));
            }
            if let Some((_, _, diff)) = &self.m3.comparison {
                ui.label(format!(
                    "Since checkpoint: {} added, {} removed, {} changed model(s).",
                    diff.added_documents.len(),
                    diff.removed_documents.len(),
                    diff.changed_documents.len()
                ));
                ui.label(format!(
                    "Dictionary: {} · resources: {} · comments: {} · links: {} · project: {}",
                    diff.dictionary_changed,
                    diff.resources_changed,
                    diff.comments_changed,
                    diff.links_changed,
                    diff.project_properties_changed
                ));
                let semantic_property_count = diff
                    .changed_properties
                    .iter()
                    .fold(0usize, |count, change| {
                        count.saturating_add(change.properties.len())
                    });
                let diagram_property_count =
                    diff.changed_diagrams.iter().fold(0usize, |count, change| {
                        count.saturating_add(change.properties.len())
                    });
                let sections = [
                    semantic_property_count,
                    diagram_property_count,
                    diff.added_elements.len(),
                    diff.removed_elements.len(),
                    diff.renamed_elements.len(),
                    diff.changed_properties.len(),
                    diff.moved_shapes.len(),
                    diff.rerouted_edges.len(),
                    diff.added_links
                        .len()
                        .saturating_add(diff.removed_links.len())
                        .saturating_add(diff.added_dictionary_links.len())
                        .saturating_add(diff.removed_dictionary_links.len()),
                ];
                let pages = sections
                    .into_iter()
                    .max()
                    .unwrap_or(0)
                    .div_ceil(COMPARISON_PAGE_SIZE)
                    .max(1);
                self.m3.comparison_page = self.m3.comparison_page.min(pages - 1);
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(
                            self.m3.comparison_page > 0,
                            egui::Button::new("Previous comparison details"),
                        )
                        .clicked()
                    {
                        self.m3.comparison_page -= 1;
                    }
                    ui.label(format!(
                        "Details page {} of {} · 100 entries per section",
                        self.m3.comparison_page + 1,
                        pages
                    ));
                    if ui
                        .add_enabled(
                            self.m3.comparison_page + 1 < pages,
                            egui::Button::new("Next comparison details"),
                        )
                        .clicked()
                    {
                        self.m3.comparison_page += 1;
                    }
                });
                let frozen = snap
                    .project
                    .checkpoints
                    .iter()
                    .find(|checkpoint| checkpoint.id == id);
                for (title, changes) in [
                    ("Added elements", &diff.added_elements),
                    ("Removed elements", &diff.removed_elements),
                    ("Renamed elements", &diff.renamed_elements),
                    ("Changed properties", &diff.changed_properties),
                ] {
                    egui::CollapsingHeader::new(format!("{title} · {}", changes.len())).show(
                        ui,
                        |ui| {
                            // Keep one frame bounded; CLI/MCP expose the full structured comparison.
                            for (index, change) in changes
                                .iter()
                                .enumerate()
                                .skip(self.m3.comparison_page * COMPARISON_PAGE_SIZE)
                                .take(COMPARISON_PAGE_SIZE)
                            {
                                let document = snap
                                    .project
                                    .documents
                                    .iter()
                                    .find(|document| document.id == change.key.document_id)
                                    .or_else(|| {
                                        frozen.and_then(|checkpoint| {
                                            checkpoint.content.documents.iter().find(|document| {
                                                document.id == change.key.document_id
                                            })
                                        })
                                    });
                                let owner = change
                                    .after_name
                                    .as_deref()
                                    .filter(|name| !name.is_empty())
                                    .or_else(|| {
                                        change
                                            .before_name
                                            .as_deref()
                                            .filter(|name| !name.is_empty())
                                    })
                                    .map(str::to_owned)
                                    .or_else(|| {
                                        document
                                            .and_then(|document| {
                                                processcraft_core::queries::element_identity(
                                                    document,
                                                    &change.key.xml_id,
                                                )
                                            })
                                            .and_then(|identity| identity.xml_id())
                                            .map(ToString::to_string)
                                    })
                                    .unwrap_or_else(|| format!("Element {}", index + 1));
                                ui.label(format!(
                                    "{} · {owner} · {} → {}",
                                    document.map_or("Unavailable model", |document| document
                                        .name
                                        .as_str()),
                                    change.before_name.as_deref().unwrap_or("—"),
                                    change.after_name.as_deref().unwrap_or("—")
                                ));
                            }
                        },
                    );
                }
                egui::CollapsingHeader::new(format!(
                    "Semantic property details · {semantic_property_count}"
                ))
                .show(ui, |ui| {
                    for (index, change, property) in
                        semantic_property_page(&diff.changed_properties, self.m3.comparison_page)
                    {
                        let document = snap
                            .project
                            .documents
                            .iter()
                            .find(|document| document.id == change.key.document_id)
                            .or_else(|| {
                                frozen.and_then(|checkpoint| {
                                    checkpoint
                                        .content
                                        .documents
                                        .iter()
                                        .find(|document| document.id == change.key.document_id)
                                })
                            });
                        let owner = change
                            .after_name
                            .as_deref()
                            .filter(|name| !name.is_empty())
                            .or_else(|| {
                                change
                                    .before_name
                                    .as_deref()
                                    .filter(|name| !name.is_empty())
                            })
                            .map(str::to_owned)
                            .or_else(|| {
                                document
                                    .and_then(|document| {
                                        processcraft_core::queries::element_identity(
                                            document,
                                            &change.key.xml_id,
                                        )
                                    })
                                    .and_then(|identity| identity.xml_id())
                                    .map(ToString::to_string)
                            })
                            .unwrap_or_else(|| format!("Element {}", index + 1));
                        ui.push_id(("semantic", index), |ui| {
                            ui.separator();
                            ui.label(format!(
                                "{} · {owner}",
                                document
                                    .map_or("Unavailable model", |document| document.name.as_str())
                            ));
                            property_detail_ui(ui, property);
                        });
                    }
                });
                egui::CollapsingHeader::new(format!(
                    "Diagram property details · {diagram_property_count}"
                ))
                .show(ui, |ui| {
                    for (index, change, property) in
                        diagram_property_page(&diff.changed_diagrams, self.m3.comparison_page)
                    {
                        let document = snap
                            .project
                            .documents
                            .iter()
                            .find(|document| document.id == change.document_id)
                            .or_else(|| {
                                frozen.and_then(|checkpoint| {
                                    checkpoint
                                        .content
                                        .documents
                                        .iter()
                                        .find(|document| document.id == change.document_id)
                                })
                            });
                        let label = document.map_or_else(
                            || "Unavailable diagram".into(),
                            |document| diagram_label(document, &change.diagram_id),
                        );
                        ui.push_id(("diagram", index), |ui| {
                            ui.separator();
                            ui.label(format!(
                                "{} · {label}",
                                document
                                    .map_or("Unavailable model", |document| document.name.as_str())
                            ));
                            property_detail_ui(ui, property);
                        });
                    }
                });
                for (title, changes) in [
                    ("Moved shapes", &diff.moved_shapes),
                    ("Rerouted connections", &diff.rerouted_edges),
                ] {
                    egui::CollapsingHeader::new(format!("{title} · {}", changes.len())).show(
                        ui,
                        |ui| {
                            for change in
                                changes.iter().skip(self.m3.comparison_page * 100).take(100)
                            {
                                let document =
                                    snap.project
                                        .documents
                                        .iter()
                                        .find(|document| document.id == change.document_id)
                                        .or_else(|| {
                                            frozen.and_then(|checkpoint| {
                                                checkpoint.content.documents.iter().find(
                                                    |document| document.id == change.document_id,
                                                )
                                            })
                                        });
                                let diagram = document.map_or_else(
                                    || "Unavailable diagram".into(),
                                    |document| diagram_label(document, &change.diagram_id),
                                );
                                let element = document
                                    .and_then(|document| {
                                        processcraft_core::queries::element_name(
                                            document,
                                            &change.element_id,
                                        )
                                    })
                                    .filter(|name| !name.is_empty())
                                    .map(str::to_owned)
                                    .unwrap_or_else(|| "Unnamed element".into());
                                let representation = document.map_or_else(
                                    || "Unavailable representation".into(),
                                    |document| {
                                        diagram_representation_label(
                                            document,
                                            &change.diagram_id,
                                            &change.representation_id,
                                        )
                                    },
                                );
                                ui.label(format!(
                                    "{} · {diagram} · {element} · {representation}",
                                    document.map_or("Unavailable model", |document| document
                                        .name
                                        .as_str())
                                ));
                            }
                        },
                    );
                }
                let link_changes = diff.added_links.len()
                    + diff.removed_links.len()
                    + diff.added_dictionary_links.len()
                    + diff.removed_dictionary_links.len();
                egui::CollapsingHeader::new(format!("Changed links · {link_changes}")).show(
                    ui,
                    |ui| {
                        let navigation = diff
                            .added_links
                            .iter()
                            .map(|link| ("Added", link))
                            .chain(diff.removed_links.iter().map(|link| ("Removed", link)))
                            .map(|(action, link)| {
                                format!(
                                    "{action} · {} / {} → {} · {}",
                                    link.source.document_id,
                                    link.source.xml_id,
                                    link.target,
                                    link.label
                                )
                            });
                        let dictionary = diff
                            .added_dictionary_links
                            .iter()
                            .map(|link| ("Added", link))
                            .chain(
                                diff.removed_dictionary_links
                                    .iter()
                                    .map(|link| ("Removed", link)),
                            )
                            .map(|(action, link)| {
                                format!(
                                    "{action} · {} / {} · {:?} → {:?}",
                                    link.source.document_id,
                                    link.source.xml_id,
                                    link.kind,
                                    link.target
                                )
                            });
                        for label in navigation
                            .chain(dictionary)
                            .skip(self.m3.comparison_page.saturating_mul(COMPARISON_PAGE_SIZE))
                            .take(COMPARISON_PAGE_SIZE)
                        {
                            ui.label(label);
                        }
                    },
                );
                ui.label(format!(
                    "Moved shapes: {} · rerouted edges: {} · model properties: {}",
                    diff.moved_shapes.len(),
                    diff.rerouted_edges.len(),
                    diff.changed_document_properties.len()
                ));
                ui.label(format!(
                    "Changed diagrams (geometry, labels or styles): {}",
                    diff.changed_diagrams.len()
                ));
            }
            ui.horizontal_wrapped(|ui| {
                if ui.button("View frozen content…").clicked() {
                    self.m3.reference_view = Some((id, 0));
                }
                if ui.button("Set as local reference").clicked() {
                    self.ctl
                        .mutate(vec![Mutation::SetReferenceCheckpoint { id: Some(id) }]);
                }
                if ui.button("Rename checkpoint").clicked() {
                    self.ctl.mutate(vec![Mutation::RenameCheckpoint {
                        id,
                        name: self.m3.checkpoint_name.clone(),
                    }]);
                }
                if ui.button("Restore checkpoint…").clicked() {
                    self.m3.restore = Some(id);
                }
                if ui
                    .add_enabled(
                        snap.project.reference_checkpoint != Some(id),
                        egui::Button::new("Delete checkpoint"),
                    )
                    .clicked()
                    && self.ctl.mutate(vec![Mutation::DeleteCheckpoint { id }])
                {
                    self.m3.checkpoint = None;
                }
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn comparison_property_pages_bound_both_owner_types() {
        use processcraft_core::queries::{DiagramChange, ElementChange, PropertyDifference};
        let app = ProcessCraft::default();
        let snapshot = app.ctl.session.snapshot();
        let document = &snapshot.project.documents[0];
        let properties = (0..251)
            .map(|i| PropertyDifference {
                property: format!("field/{i}"),
                before: serde_json::Value::Null,
                after: serde_json::json!(i),
            })
            .collect::<Vec<_>>();
        let semantic = vec![ElementChange {
            key: ElementKey {
                document_id: document.id,
                xml_id: document.processes[0].id.selector(),
            },
            before_name: None,
            after_name: None,
            properties: properties.clone(),
        }];
        let diagrams = vec![DiagramChange {
            document_id: document.id,
            diagram_id: document.diagrams[0].id.selector(),
            properties,
        }];
        assert_eq!(semantic_property_page(&semantic, 0).count(), 100);
        assert_eq!(semantic_property_page(&semantic, 1).next().unwrap().0, 100);
        assert_eq!(semantic_property_page(&semantic, 2).count(), 51);
        assert_eq!(diagram_property_page(&diagrams, 0).count(), 100);
        assert_eq!(diagram_property_page(&diagrams, 1).next().unwrap().0, 100);
        assert_eq!(diagram_property_page(&diagrams, 2).count(), 51);
    }
    #[test]
    fn checkpoint_plane_changes_reach_diagram_detail_pages() {
        let mut app = ProcessCraft::default();
        let snapshot = app.ctl.session.snapshot();
        let document_id = snapshot.project.documents[0].id;
        let diagram_id = snapshot.project.documents[0].diagrams[0].id.selector();
        let checkpoint = CheckpointId::new();
        assert!(app.ctl.mutate(vec![Mutation::CreateCheckpoint {
            id: checkpoint,
            name: "Before".into(),
            created_unix_ms: 1
        }]));
        assert!(app.ctl.mutate(vec![Mutation::SetDiagramPlaneMetadata {
            document_id,
            diagram_id,
            metadata: Some(processcraft_core::model::DiElementMetadata::default())
        }]));
        let comparison = processcraft_core::queries::compare_checkpoint(
            &app.ctl.session.snapshot().project,
            checkpoint,
        )
        .unwrap();
        let rows = diagram_property_page(&comparison.changed_diagrams, 0).collect::<Vec<_>>();
        assert!(!rows.is_empty());
        assert!(
            rows.iter()
                .any(|(_, _, property)| property.property.contains("plane_metadata"))
        );
    }
    #[test]
    fn comparison_value_preview_is_bounded_and_copy_remains_complete() {
        let text = "界".repeat(200_000);
        let value = serde_json::Value::String(text.clone());
        let (preview, short) = value_preview(&value);
        assert!(short);
        assert_eq!(preview.chars().count(), 500);
        assert_eq!(copy_value(&value), text);
        let nested = serde_json::json!({"large":"x".repeat(2_000_000)});
        let (preview, short) = value_preview(&nested);
        assert!(short);
        assert!(preview.chars().count() <= 500);
        assert!(copy_value(&nested).len() > 2_000_000);
        assert_eq!(
            value_preview(&serde_json::Value::Null),
            ("Unset".into(), false)
        );
    }
    #[test]
    fn frozen_details_can_reach_the_second_page() {
        let mut app = ProcessCraft::default();
        let snapshot = app.ctl.session.snapshot();
        let target = ElementKey {
            document_id: snapshot.project.documents[0].id,
            xml_id: snapshot.project.documents[0].processes[0].id.selector(),
        };
        assert!(
            app.ctl.mutate(
                (0..101)
                    .map(|i| Mutation::CreateComment {
                        comment: Comment {
                            id: CommentId::new(),
                            target: target.clone(),
                            author: "Analyst".into(),
                            text: format!("Comment {i}")
                        }
                    })
                    .collect()
            )
        );
        let id = CheckpointId::new();
        assert!(app.ctl.mutate(vec![Mutation::CreateCheckpoint {
            id,
            name: "Reference".into(),
            created_unix_ms: 1
        }]));
        app.m3.reference_view = Some((id, 0));
        let context = egui::Context::default();
        let frame = |app: &mut ProcessCraft, events| {
            context.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1440.0, 900.0),
                    )),
                    events,
                    ..Default::default()
                },
                |_ui| app.reference_window(&context),
            )
        };
        let mut output = frame(&mut app, vec![]);
        output.textures_delta.clear();
        let mut output = frame(&mut app, vec![]);
        output.textures_delta.clear();
        let next = output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.job.text == "Next frozen details" => {
                    Some(text.pos + text.galley.size() * 0.5)
                }
                _ => None,
            })
            .expect("next-page button is visible");
        let button = |pressed| egui::Event::PointerButton {
            pos: next,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        let mut output = frame(
            &mut app,
            vec![egui::Event::PointerMoved(next), button(true)],
        );
        output.textures_delta.clear();
        let mut output = frame(&mut app, vec![button(false)]);
        output.textures_delta.clear();
        assert_eq!(app.m3.reference_page, 1);
    }
    #[test]
    fn layout_preview_does_not_replace_the_editor_camera() {
        let mut app = ProcessCraft::default();
        let context = egui::Context::default();
        let frame = |app: &mut ProcessCraft| {
            let mut output = context.run_ui(egui::RawInput::default(), |ui| app.canvas_ui(ui));
            output.textures_delta.clear();
        };
        frame(&mut app);
        let snapshot = app.ctl.session.snapshot();
        let document = snapshot.project.documents[0].clone();
        let proposal = LayoutProposal {
            session_id: app.ctl.session.id(),
            generation: snapshot.generation,
            document_id: document.id,
            diagram_id: document.diagrams[0].id.selector(),
            original: processcraft_core::layout::LayoutGeometry {
                shapes: vec![],
                edges: vec![],
            },
            proposed: processcraft_core::layout::LayoutGeometry {
                shapes: vec![],
                edges: vec![],
            },
        };
        app.camera = (312.0, -125.0);
        app.zoom = 2.5;
        app.m3.preview = Some((proposal, document));
        frame(&mut app);
        assert_eq!(app.camera, (312.0, -125.0));
        assert_eq!(app.zoom, 2.5);
        app.m3.preview = None;
        frame(&mut app);
        assert_eq!(app.camera, (312.0, -125.0));
        assert_eq!(app.zoom, 2.5);
    }
    #[test]
    fn cancelled_queued_layout_and_comment_drafts_keep_project_unchanged() {
        let mut app = ProcessCraft::default();
        let before = app.ctl.session.snapshot();
        app.ctl.session.mark_saved(before.content_id);
        app.m3.session = app.ctl.session.id();
        let doc = &before.project.documents[0];
        let proposal = LayoutProposal {
            session_id: app.ctl.session.id(),
            generation: before.generation,
            document_id: doc.id,
            diagram_id: doc.diagrams[0].id.selector(),
            original: processcraft_core::layout::LayoutGeometry {
                shapes: vec![],
                edges: vec![],
            },
            proposed: processcraft_core::layout::LayoutGeometry {
                shapes: vec![],
                edges: vec![],
            },
        };
        let (tx, rx) = mpsc::channel();
        tx.send(Ok(proposal)).unwrap();
        app.m3.worker = Some(rx);
        app.m3.cancel = Some(Arc::new(AtomicBool::new(true)));
        app.m3_poll(&egui::Context::default());
        assert!(app.m3.preview.is_none());
        assert!(app.m3.worker.is_none());
        let session = app.ctl.session.id();
        app.m3.text = "Unposted local note".into();
        app.new_or_open("new");
        assert_eq!(app.confirm, Some("new"));
        assert_eq!(app.ctl.session.id(), session);
        assert_eq!(app.ctl.session.snapshot().content_id, before.content_id);
        assert_eq!(app.m3.text, "Unposted local note");
    }
}
