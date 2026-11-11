use super::ProcessCraft;
use crate::widgets::{self, Icon};
use egui_ltreeview::{Action, NodeBuilder, TreeView, TreeViewState};
use processcraft_core::{
    ids::{DocumentId, XmlId},
    model::ProjectData,
};
use std::collections::{HashMap, HashSet};

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum TreeId {
    Root,
    Folder(XmlId),
    Document(DocumentId),
}

#[derive(Clone, Debug)]
enum Entry {
    Open(TreeId, String),
    Leaf(TreeId, String),
    Close,
}

// Iterative traversal tolerates malformed parent links and never recurses into imported data.
fn entries(project: &ProjectData, search: &str) -> Vec<Entry> {
    let query = search.trim().to_lowercase();
    let folders: HashMap<_, _> = project.folders.iter().map(|f| (f.id.clone(), f)).collect();
    let mut included = HashSet::new();
    for folder in &project.folders {
        if query.is_empty() || folder.name.to_lowercase().contains(&query) {
            included.insert(folder.id.clone());
        }
    }
    for doc in &project.documents {
        if (query.is_empty() || doc.name.to_lowercase().contains(&query))
            && let Some(id) = &doc.folder
        {
            included.insert(id.clone());
        }
    }
    let mut pending: Vec<_> = included.iter().cloned().collect();
    while let Some(id) = pending.pop() {
        if let Some(parent) = folders.get(&id).and_then(|f| f.parent.as_ref())
            && folders.contains_key(parent)
            && included.insert(parent.clone())
        {
            pending.push(parent.clone());
        }
    }
    let mut children: HashMap<Option<XmlId>, Vec<&processcraft_core::model::Folder>> =
        HashMap::new();
    for folder in &project.folders {
        if included.contains(&folder.id) {
            // Bound visual nesting even for a very deep imported hierarchy. Every
            // eighth level starts a new visible branch; no model is hidden.
            let mut cursor = folder.parent.as_ref();
            let mut depth = 0;
            let mut seen = HashSet::new();
            while let Some(id) = cursor {
                if !seen.insert(id.clone()) {
                    break;
                }
                depth += 1;
                cursor = folders.get(id).and_then(|f| f.parent.as_ref());
            }
            let parent = if depth > 0 && depth % 8 == 0 {
                None
            } else {
                folder.parent.clone().filter(|id| folders.contains_key(id))
            };
            children.entry(parent).or_default().push(folder);
        }
    }
    let mut docs: HashMap<Option<XmlId>, Vec<_>> = HashMap::new();
    for doc in &project.documents {
        if query.is_empty() || doc.name.to_lowercase().contains(&query) {
            let parent = doc.folder.clone().filter(|id| folders.contains_key(id));
            docs.entry(parent).or_default().push(doc);
        }
    }
    enum Work {
        Children(Option<XmlId>),
        Folder(XmlId, String),
        Close,
    }
    let mut result = vec![Entry::Open(TreeId::Root, "All models".into())];
    let mut stack = vec![Work::Close, Work::Children(None)];
    let mut visited = HashSet::new();
    while let Some(work) = stack.pop() {
        match work {
            Work::Close => result.push(Entry::Close),
            Work::Folder(id, name) => {
                if !visited.insert(id.clone()) {
                    continue;
                }
                result.push(Entry::Open(TreeId::Folder(id.clone()), name));
                stack.push(Work::Close);
                stack.push(Work::Children(Some(id)));
            }
            Work::Children(parent) => {
                if let Some(items) = docs.get(&parent) {
                    for doc in items {
                        result.push(Entry::Leaf(TreeId::Document(doc.id), doc.name.clone()));
                    }
                }
                if let Some(items) = children.get(&parent) {
                    for folder in items.iter().rev() {
                        stack.push(Work::Folder(folder.id.clone(), folder.name.clone()));
                    }
                }
            }
        }
    }
    result
}

impl ProcessCraft {
    pub(super) fn project_tree_ui(&mut self, ui: &mut egui::Ui, search: &str) {
        let snap = self.ctl.session.snapshot();
        let rows = entries(&snap.project, search);
        let id = egui::Id::unique(("project-tree", self.ctl.session.id()));
        let mut state = TreeViewState::<TreeId>::load(ui, id).unwrap_or_else(|| {
            let mut state = TreeViewState::default();
            state.set_openness(TreeId::Root, true);
            state
        });
        let current = if matches!(self.view, 0 | 3) {
            snap.project
                .documents
                .get(self.doc)
                .map(|d| TreeId::Document(d.id))
        } else if self.view == 1 {
            Some(
                self.folder_selected
                    .clone()
                    .map_or(TreeId::Root, TreeId::Folder),
            )
        } else {
            None
        };
        state.set_selected(current.clone().into_iter().collect());
        let active_id = egui::Id::unique(("project-tree-active", self.ctl.session.id()));
        let previous = ui.ctx().data(|data| data.get_temp::<TreeId>(active_id));
        if current != previous {
            if let Some(TreeId::Document(doc_id)) = &current {
                let mut parent = snap
                    .project
                    .documents
                    .iter()
                    .find(|d| &d.id == doc_id)
                    .and_then(|d| d.folder.clone());
                let mut seen = HashSet::new();
                state.set_openness(TreeId::Root, true);
                while let Some(id) = parent {
                    if !seen.insert(id.clone()) {
                        break;
                    }
                    state.set_openness(TreeId::Folder(id.clone()), true);
                    parent = snap
                        .project
                        .folders
                        .iter()
                        .find(|f| f.id == id)
                        .and_then(|f| f.parent.clone());
                }
            }
            ui.ctx().data_mut(|data| {
                if let Some(active) = current.clone() {
                    data.insert_temp(active_id, active);
                } else {
                    data.remove::<TreeId>(active_id);
                }
            });
        }
        if !search.trim().is_empty() {
            for row in &rows {
                if let Entry::Open(id, _) = row {
                    state.set_openness(id.clone(), true);
                }
            }
        }
        let color = self.theme.palette().muted;
        let mut clicked = None;
        let mut focusable = Vec::new();
        let (_, actions) = TreeView::new(id)
            .allow_multi_selection(false)
            .allow_drag_and_drop(false)
            .default_node_height(Some(32.0))
            .show_state(ui, &mut state, |tree| {
                for row in &rows {
                    match row {
                        Entry::Open(id, label) => {
                            tree.node(NodeBuilder::dir(id.clone()).label_ui(|ui| {
                                ui.horizontal(|ui| {
                                    widgets::icon(ui, Icon::Folder, 16.0, color);
                                    let width = ui.available_width().min(
                                        (ui.clip_rect().right() - ui.cursor().left()).max(24.0),
                                    );
                                    let response = ui
                                        .scope(|ui| {
                                            ui.set_max_width(width);
                                            ui.add(
                                                egui::Button::selectable(
                                                    current.as_ref() == Some(id),
                                                    label,
                                                )
                                                .truncate(),
                                            )
                                        })
                                        .inner
                                        .on_hover_text(label);
                                    ui.memory_mut(|memory| {
                                        memory.set_focus_lock_filter(
                                            response.id,
                                            egui::EventFilter {
                                                tab: false,
                                                escape: false,
                                                horizontal_arrows: true,
                                                vertical_arrows: true,
                                            },
                                        )
                                    });
                                    focusable.push((id.clone(), response.id, response.has_focus()));
                                    if response.clicked() {
                                        clicked = Some(id.clone());
                                    }
                                });
                            }));
                        }
                        Entry::Leaf(id, label) => {
                            tree.node(NodeBuilder::leaf(id.clone()).label_ui(|ui| {
                                ui.horizontal(|ui| {
                                    widgets::icon(ui, Icon::Workflow, 16.0, color);
                                    let width = ui.available_width().min(
                                        (ui.clip_rect().right() - ui.cursor().left()).max(24.0),
                                    );
                                    let response = ui
                                        .scope(|ui| {
                                            ui.set_max_width(width);
                                            ui.add(
                                                egui::Button::selectable(
                                                    current.as_ref() == Some(id),
                                                    label,
                                                )
                                                .truncate(),
                                            )
                                        })
                                        .inner
                                        .on_hover_text(label);
                                    ui.memory_mut(|memory| {
                                        memory.set_focus_lock_filter(
                                            response.id,
                                            egui::EventFilter {
                                                tab: false,
                                                escape: false,
                                                horizontal_arrows: true,
                                                vertical_arrows: true,
                                            },
                                        )
                                    });
                                    focusable.push((id.clone(), response.id, response.has_focus()));
                                    if response.clicked() {
                                        clicked = Some(id.clone());
                                    }
                                });
                            }));
                        }
                        Entry::Close => tree.close_dir(),
                    }
                }
            });
        let tree_has_focus = ui.memory(|memory| memory.has_focus(id));
        let focused_index = focusable
            .iter()
            .position(|(_, _, focused)| *focused)
            .or_else(|| {
                if tree_has_focus {
                    focusable
                        .iter()
                        .position(|(node, _, _)| current.as_ref() == Some(node))
                } else {
                    None
                }
            });
        if let Some(target) = &clicked
            && let Some((_, focus_id, _)) = focusable.iter().find(|(node, _, _)| node == target)
        {
            ui.memory_mut(|memory| memory.request_focus(*focus_id));
        }
        if clicked.is_none()
            && let Some(index) = focused_index
        {
            if matches!(focusable[index].0, TreeId::Root | TreeId::Folder(_)) {
                let openness = ui.input(|input| {
                    if input.key_pressed(egui::Key::ArrowRight) {
                        Some(true)
                    } else if input.key_pressed(egui::Key::ArrowLeft) {
                        Some(false)
                    } else {
                        None
                    }
                });
                if let Some(open) = openness {
                    state.set_openness(focusable[index].0.clone(), open);
                    ui.ctx().request_repaint();
                }
            }
            let delta = ui.input(|input| {
                if input.key_pressed(egui::Key::ArrowDown) {
                    1_isize
                } else if input.key_pressed(egui::Key::ArrowUp) {
                    -1_isize
                } else {
                    0
                }
            });
            if delta != 0 {
                let next = (index as isize + delta).clamp(0, focusable.len() as isize - 1) as usize;
                if !tree_has_focus {
                    ui.memory_mut(|memory| memory.request_focus(focusable[next].1));
                }
                clicked = Some(focusable[next].0.clone());
            }
        }
        let target = clicked.or_else(|| {
            actions.into_iter().find_map(|action| match action {
                Action::SetSelected(selected) => selected.into_iter().next(),
                _ => None,
            })
        });
        if let Some(target) = target.filter(|target| {
            Some(target) != current.as_ref()
                || (self.view == 3 && matches!(target, TreeId::Document(_)))
        }) && self.prepare_navigation()
        {
            match target {
                TreeId::Root => {
                    self.folder_selected = None;
                    self.view = 1;
                }
                TreeId::Folder(id) => {
                    if let Some(folder) = snap.project.folders.iter().find(|f| f.id == id) {
                        self.folder_selected = Some(id);
                        self.folder_name = folder.name.clone();
                        self.view = 1;
                    }
                }
                TreeId::Document(id) => {
                    if let Some(index) = snap.project.documents.iter().position(|d| d.id == id) {
                        self.doc = index;
                        self.process_id = None;
                        self.diagram_id = None;
                        self.select(None);
                        self.view = 0;
                    }
                }
            }
        }
        // A rejected draft guard must not leave the widget on a phantom selection.
        let selected = if matches!(self.view, 0 | 3) {
            snap.project
                .documents
                .get(self.doc)
                .map(|d| TreeId::Document(d.id))
        } else if self.view == 1 {
            Some(
                self.folder_selected
                    .clone()
                    .map_or(TreeId::Root, TreeId::Folder),
            )
        } else {
            None
        };
        state.set_selected(selected.into_iter().collect());
        state.store(ui, id);
        if !search.trim().is_empty()
            && !rows
                .iter()
                .any(|row| matches!(row, Entry::Leaf(_, _) | Entry::Open(TreeId::Folder(_), _)))
        {
            ui.label("No matching folders or models.");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use processcraft_core::model::{BpmnDocument, Folder};
    use std::sync::Arc;

    #[test]
    fn search_keeps_folder_ancestors_and_excludes_other_documents() {
        let mut project = ProjectData::new("Project".into());
        let parent: XmlId = "parent".parse().unwrap();
        let child: XmlId = "child".parse().unwrap();
        project.folders.push(Folder {
            id: parent.clone(),
            name: "Parent".into(),
            parent: None,
        });
        project.folders.push(Folder {
            id: child.clone(),
            name: "Child".into(),
            parent: Some(parent.clone()),
        });
        let mut target = BpmnDocument::new("Needle".into(), "defs".parse().unwrap());
        target.folder = Some(child.clone());
        let target_id = target.id;
        project.documents.push(Arc::new(target));
        project.documents.push(Arc::new(BpmnDocument::new(
            "Other".into(),
            "defs2".parse().unwrap(),
        )));
        let rows = entries(&project, "needle");
        let ids: Vec<_> = rows
            .iter()
            .filter_map(|row| match row {
                Entry::Open(id, _) | Entry::Leaf(id, _) => Some(id.clone()),
                Entry::Close => None,
            })
            .collect();
        assert_eq!(
            ids,
            vec![
                TreeId::Root,
                TreeId::Folder(parent),
                TreeId::Folder(child),
                TreeId::Document(target_id)
            ]
        );
    }
    #[test]
    fn deep_folders_remain_visible_with_bounded_nesting() {
        let mut project = ProjectData::new("Project".into());
        let mut parent = None;
        for n in 0..40 {
            let id: XmlId = format!("folder{n}").parse().unwrap();
            project.folders.push(Folder {
                id: id.clone(),
                name: format!("Folder {n}"),
                parent,
            });
            parent = Some(id);
        }
        let mut doc = BpmnDocument::new("Deep model".into(), "defs".parse().unwrap());
        doc.folder = parent;
        project.documents.push(Arc::new(doc));
        let mut depth = 0;
        let mut max_depth = 0;
        let mut folder_count = 0;
        let mut document_count = 0;
        for entry in entries(&project, "") {
            match entry {
                Entry::Open(id, _) => {
                    depth += 1;
                    max_depth = max_depth.max(depth);
                    if matches!(id, TreeId::Folder(_)) {
                        folder_count += 1;
                    }
                }
                Entry::Close => depth -= 1,
                Entry::Leaf(TreeId::Document(_), _) => document_count += 1,
                Entry::Leaf(_, _) => {}
            }
        }
        assert_eq!((folder_count, document_count), (40, 1));
        assert!(max_depth <= 9);
        assert_eq!(depth, 0);
    }
    #[test]
    fn arrow_down_moves_from_focused_root_to_document() {
        let mut app = ProcessCraft {
            view: 1,
            folder_selected: None,
            ..Default::default()
        };
        let ctx = egui::Context::default();
        let draw = |ctx: &egui::Context, app: &mut ProcessCraft, events: Vec<egui::Event>| {
            let input = egui::RawInput {
                events,
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(500.0, 500.0),
                )),
                ..Default::default()
            };
            let mut output = ctx.run_ui(input, |ui| app.project_tree_ui(ui, ""));
            output.textures_delta.clear();
        };
        draw(&ctx, &mut app, vec![]);
        let key = |key| egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        };
        draw(&ctx, &mut app, vec![key(egui::Key::Tab)]);
        let state_id = egui::Id::unique(("project-tree", app.ctl.session.id()));
        draw(&ctx, &mut app, vec![key(egui::Key::ArrowLeft)]);
        let open = ctx.data(|data| {
            data.get_temp::<TreeViewState<TreeId>>(state_id)
                .unwrap()
                .is_open(&TreeId::Root)
        });
        assert_eq!(open, Some(false));
        assert_eq!(app.view, 1);
        draw(&ctx, &mut app, vec![key(egui::Key::ArrowRight)]);
        let open = ctx.data(|data| {
            data.get_temp::<TreeViewState<TreeId>>(state_id)
                .unwrap()
                .is_open(&TreeId::Root)
        });
        assert_eq!(open, Some(true));
        assert_eq!(app.view, 1);
        draw(&ctx, &mut app, vec![]);
        draw(&ctx, &mut app, vec![key(egui::Key::ArrowDown)]);
        assert_eq!(app.view, 0);
    }
    #[test]
    fn workbench_arrows_work_when_tree_container_has_focus() {
        let mut app = ProcessCraft {
            view: 1,
            folder_selected: None,
            ..Default::default()
        };
        let mut project = super::super::controller::blank_project();
        let folder_id: XmlId = "folder_test".parse().unwrap();
        project.folders.push(Folder {
            id: folder_id.clone(),
            name: "Folder".into(),
            parent: None,
        });
        Arc::make_mut(&mut project.documents[0]).folder = Some(folder_id.clone());
        app.ctl.session = processcraft_core::commands::Session::new(project).unwrap();
        let ctx = egui::Context::default();
        let draw = |ctx: &egui::Context, app: &mut ProcessCraft, events: Vec<egui::Event>| {
            let input = egui::RawInput {
                events,
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1360.0, 900.0),
                )),
                ..Default::default()
            };
            let mut output = ctx.run_ui(input, |ui| app.workbench_ui(ui));
            output.textures_delta.clear();
        };
        let key = |key| egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        };
        draw(&ctx, &mut app, vec![]);
        let tree_id = egui::Id::unique(("project-tree", app.ctl.session.id()));
        ctx.memory_mut(|memory| memory.request_focus(tree_id));
        draw(&ctx, &mut app, vec![]);
        draw(&ctx, &mut app, vec![key(egui::Key::ArrowLeft)]);
        let open = ctx.data(|data| {
            data.get_temp::<TreeViewState<TreeId>>(tree_id)
                .unwrap()
                .is_open(&TreeId::Root)
        });
        assert_eq!(open, Some(false));
        draw(&ctx, &mut app, vec![]);
        draw(&ctx, &mut app, vec![key(egui::Key::ArrowRight)]);
        let open = ctx.data(|data| {
            data.get_temp::<TreeViewState<TreeId>>(tree_id)
                .unwrap()
                .is_open(&TreeId::Root)
        });
        assert_eq!(open, Some(true));
        draw(&ctx, &mut app, vec![]);
        draw(&ctx, &mut app, vec![key(egui::Key::ArrowDown)]);
        assert_eq!(app.view, 1);
        assert_eq!(app.folder_selected, Some(folder_id));
        draw(&ctx, &mut app, vec![]);
        draw(&ctx, &mut app, vec![key(egui::Key::ArrowDown)]);
        assert_eq!(app.view, 0);
    }
}
