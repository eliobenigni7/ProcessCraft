use super::catalog::optional_text;
use super::event::{ReferenceDraft, build_ref, local_choice_label, ref_ui};
use super::inspector::{ExpressionDefault, optional_expression_ui_with_default};
use super::*;
use processcraft_core::bpmn_activity::{
    ResourceAssignmentExpression, ResourceParameterBinding, ResourceRole, ResourceRoleAssignment,
    ResourceRoleKind,
};

#[derive(Clone)]
struct ParameterDraft {
    value: ResourceParameterBinding,
    reference: Option<ReferenceDraft>,
}
#[derive(Clone)]
enum AssignmentDraft {
    None,
    Resource {
        reference: Option<ReferenceDraft>,
        bindings: Vec<ParameterDraft>,
    },
    Expression(Box<ResourceAssignmentExpression>),
}
#[derive(Clone)]
struct RoleDraft {
    value: ResourceRole,
    assignment: AssignmentDraft,
}
#[derive(Clone, Default)]
pub(super) struct RolesDraft {
    roles: Vec<RoleDraft>,
}
impl RolesDraft {
    pub(super) fn new(values: &[ResourceRole]) -> Self {
        Self {
            roles: values
                .iter()
                .map(|value| RoleDraft {
                    value: value.clone(),
                    assignment: match &value.assignment {
                        ResourceRoleAssignment::None => AssignmentDraft::None,
                        ResourceRoleAssignment::Resource {
                            reference,
                            bindings,
                        } => AssignmentDraft::Resource {
                            reference: Some(ReferenceDraft::new(reference)),
                            bindings: bindings
                                .iter()
                                .map(|b| ParameterDraft {
                                    value: b.clone(),
                                    reference: b.parameter_ref.as_ref().map(ReferenceDraft::new),
                                })
                                .collect(),
                        },
                        ResourceRoleAssignment::Expression { assignment } => {
                            AssignmentDraft::Expression(Box::new(assignment.clone()))
                        }
                    },
                })
                .collect(),
        }
    }
    pub(super) fn build(&self) -> Result<Vec<ResourceRole>, String> {
        self.roles
            .iter()
            .map(|r| {
                let mut value = r.value.clone();
                value.assignment = match &r.assignment {
                    AssignmentDraft::None => ResourceRoleAssignment::None,
                    AssignmentDraft::Expression(assignment) => ResourceRoleAssignment::Expression {
                        assignment: assignment.as_ref().clone(),
                    },
                    AssignmentDraft::Resource {
                        reference,
                        bindings,
                    } => ResourceRoleAssignment::Resource {
                        reference: build_ref(reference)?
                            .ok_or("Select a resource or enter a qualified resource reference.")?,
                        bindings: bindings
                            .iter()
                            .map(|b| {
                                let mut value = b.value.clone();
                                value.parameter_ref = build_ref(&b.reference)?;
                                Ok(value)
                            })
                            .collect::<Result<_, String>>()?,
                    },
                };
                Ok(value)
            })
            .collect()
    }
    pub(super) fn ui(
        &mut self,
        ui: &mut egui::Ui,
        document: &BpmnDocument,
        expression_draft: &mut Option<super::inspector::ExpressionDraft>,
        slot: &str,
    ) {
        let resources: Vec<_> = document
            .catalog
            .resources
            .iter()
            .enumerate()
            .filter_map(|(i, r)| {
                r.id.as_ref().map(|id| {
                    (
                        id.selector(),
                        local_choice_label(Some(&r.name), "Resource", id, i + 1),
                    )
                })
            })
            .collect();
        let mut remove = None;
        let mut up = None;
        for (i, role) in self.roles.iter_mut().enumerate() {
            ui.push_id(i, |ui| {
                ui.separator();
                ui.strong(format!("Role {}", i + 1));
                optional_text(ui, "Role name", &mut role.value.name);
                egui::ComboBox::from_label("Role type")
                    .selected_text(match role.value.kind {
                        ResourceRoleKind::ResourceRole => "Resource role",
                        ResourceRoleKind::Performer => "Performer",
                        ResourceRoleKind::HumanPerformer => "Human performer",
                        ResourceRoleKind::PotentialOwner => "Potential owner",
                    })
                    .show_ui(ui, |ui| {
                        for (value, label) in [
                            (ResourceRoleKind::ResourceRole, "Resource role"),
                            (ResourceRoleKind::Performer, "Performer"),
                            (ResourceRoleKind::HumanPerformer, "Human performer"),
                            (ResourceRoleKind::PotentialOwner, "Potential owner"),
                        ] {
                            ui.selectable_value(&mut role.value.kind, value, label);
                        }
                    });
                let old = match &role.assignment {
                    AssignmentDraft::None => 0,
                    AssignmentDraft::Resource { .. } => 1,
                    AssignmentDraft::Expression(_) => 2,
                };
                let mut selected = old;
                egui::ComboBox::from_label("Assignment")
                    .selected_text(["Unspecified", "Resource", "Expression"][old])
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut selected, 0, "Unspecified");
                        ui.selectable_value(&mut selected, 1, "Resource");
                        ui.selectable_value(&mut selected, 2, "Expression");
                    });
                let has_details = match &role.assignment {
                    AssignmentDraft::None => false,
                    AssignmentDraft::Resource {
                        reference,
                        bindings,
                    } => reference.is_some() || !bindings.is_empty(),
                    AssignmentDraft::Expression(assignment) => {
                        assignment.id.is_some()
                            || assignment.base.is_some()
                            || assignment.expression.is_some()
                    }
                };
                if selected != old && has_details {
                    ui.label("Clear the existing assignment details before changing its type.");
                }
                if selected != old && !has_details && expression_draft.is_none() {
                    role.assignment = match selected {
                        1 => AssignmentDraft::Resource {
                            reference: None,
                            bindings: vec![],
                        },
                        2 => AssignmentDraft::Expression(Box::new(ResourceAssignmentExpression {
                            id: None,
                            base: None,
                            expression: None,
                        })),
                        _ => AssignmentDraft::None,
                    };
                }
                match &mut role.assignment {
                    AssignmentDraft::None => {}
                    AssignmentDraft::Expression(assignment) => {
                        optional_expression_ui_with_default(
                            ui,
                            &format!("{slot} role {} assignment", i + 1),
                            &mut assignment.expression,
                            true,
                            expression_draft,
                            ExpressionDefault::Base,
                        );
                    }
                    AssignmentDraft::Resource {
                        reference,
                        bindings,
                    } => {
                        ref_ui(ui, "Resource", reference, &resources);
                        let selected = match reference {
                            Some(ReferenceDraft::Local(Some(id))) => Some(id.clone()),
                            _ => None,
                        };
                        let parameters: Vec<_> = document
                            .catalog
                            .resources
                            .iter()
                            .filter(|r| {
                                r.id.as_ref()
                                    .is_some_and(|id| Some(id.selector()) == selected)
                            })
                            .flat_map(|r| r.parameters.iter())
                            .enumerate()
                            .filter_map(|(i, p)| {
                                p.id.as_ref().map(|id| {
                                    (
                                        id.selector(),
                                        local_choice_label(
                                            p.name.as_deref(),
                                            "Parameter",
                                            id,
                                            i + 1,
                                        ),
                                    )
                                })
                            })
                            .collect();
                        let mut delete = None;
                        let mut up = None;
                        for (j, binding) in bindings.iter_mut().enumerate() {
                            ui.push_id(j, |ui| {
                                ui.label(format!("Parameter binding {}", j + 1));
                                ref_ui(ui, "Parameter", &mut binding.reference, &parameters);
                                optional_expression_ui_with_default(
                                    ui,
                                    &format!("{slot} role {} parameter {}", i + 1, j + 1),
                                    &mut binding.value.expression,
                                    true,
                                    expression_draft,
                                    ExpressionDefault::Base,
                                );
                                if ui
                                    .add_enabled(
                                        expression_draft.is_none() && j > 0,
                                        egui::Button::new("Move binding up"),
                                    )
                                    .clicked()
                                {
                                    up = Some(j);
                                }
                                if ui
                                    .add_enabled(
                                        expression_draft.is_none(),
                                        egui::Button::new("Remove binding"),
                                    )
                                    .clicked()
                                {
                                    delete = Some(j);
                                }
                            });
                        }
                        if let Some(j) = delete {
                            bindings.remove(j);
                        } else if let Some(j) = up {
                            bindings.swap(j, j - 1);
                        }
                        if ui
                            .add_enabled(
                                expression_draft.is_none() && bindings.len() < 256,
                                egui::Button::new("Add parameter binding"),
                            )
                            .clicked()
                        {
                            bindings.push(ParameterDraft {
                                value: ResourceParameterBinding {
                                    id: None,
                                    base: None,
                                    parameter_ref: None,
                                    expression: None,
                                },
                                reference: None,
                            });
                        }
                    }
                }
                ui.add_enabled_ui(expression_draft.is_none(), |ui| {
                    if ui
                        .add_enabled(i > 0, egui::Button::new("Move role up"))
                        .clicked()
                    {
                        up = Some(i);
                    }
                    if ui.button("Remove role").clicked() {
                        remove = Some(i);
                    }
                });
            });
        }
        if let Some(i) = remove {
            self.roles.remove(i);
        } else if let Some(i) = up {
            self.roles.swap(i, i - 1);
        }
        if ui
            .add_enabled(
                expression_draft.is_none() && self.roles.len() < 256,
                egui::Button::new("Add role"),
            )
            .clicked()
        {
            self.roles.push(RoleDraft {
                value: ResourceRole {
                    id: None,
                    base: None,
                    name: None,
                    kind: ResourceRoleKind::ResourceRole,
                    assignment: ResourceRoleAssignment::None,
                },
                assignment: AssignmentDraft::None,
            });
        }
    }
}

pub(super) struct ActivityRolesDraft {
    target: ElementSelector,
    value: processcraft_core::bpmn_activity::ActivityProperties,
    roles: RolesDraft,
    properties: Vec<super::event::PropertyDraft>,
    error: String,
}
impl ProcessCraft {
    pub(super) fn activity_roles_ui(
        &mut self,
        ui: &mut egui::Ui,
        node: &processcraft_core::model::Node,
        document: &BpmnDocument,
        editable: bool,
    ) {
        if self.activity_roles_draft.is_none() {
            if ui
                .add_enabled(
                    editable && !self.draft_dirty,
                    egui::Button::new("Edit roles and properties…"),
                )
                .clicked()
                && self.commit_draft()
            {
                let value = node.activity.clone().unwrap_or_default();
                self.activity_roles_draft = Some(ActivityRolesDraft {
                    target: node.id.selector(),
                    roles: RolesDraft::new(&value.resource_roles),
                    properties: value
                        .properties
                        .iter()
                        .map(super::event::PropertyDraft::new)
                        .collect(),
                    value,
                    error: String::new(),
                });
            }
            return;
        }
        let draft = self.activity_roles_draft.as_mut().unwrap();
        if draft.target != node.id.selector() {
            return;
        }
        ui.strong("Activity roles and properties");
        ui.collapsing("Properties", |ui| {
            let mut remove = None;
            let mut up = None;
            for (i, property) in draft.properties.iter_mut().enumerate() {
                ui.push_id(i, |ui| {
                    ui.label(format!("Property {}", i + 1));
                    super::catalog::optional_text(ui, "Name", &mut property.name);
                    super::data::item_ui(ui, &mut property.item);
                    if ui
                        .add_enabled(i > 0, egui::Button::new("Move property up"))
                        .clicked()
                    {
                        up = Some(i);
                    }
                    if ui.button("Remove property").clicked() {
                        remove = Some(i);
                    }
                });
            }
            if let Some(i) = remove {
                draft.properties.remove(i);
            } else if let Some(i) = up {
                draft.properties.swap(i, i - 1);
            }
            if ui
                .add_enabled(
                    draft.properties.len() < 256,
                    egui::Button::new("Add property"),
                )
                .clicked()
            {
                draft.properties.push(super::event::PropertyDraft::new(
                    &processcraft_core::bpmn_model::EventProperty {
                        id: None,
                        base: None,
                        name: None,
                        item_aware: None,
                    },
                ));
            }
        });
        if node.kind == NodeKind::UserTask {
            ui.collapsing("Renderings", |ui| {
                let mut remove = None;
                let mut up = None;
                for (i, rendering) in draft.value.renderings.iter().enumerate() {
                    ui.push_id(i, |ui| {
                        ui.label(format!("Rendering {}", i + 1));
                        if let Some(id) = rendering.id.as_ref().and_then(|id| id.xml_id()) {
                            ui.label(format!("XML ID · {id}"));
                        }
                        if ui
                            .add_enabled(i > 0, egui::Button::new("Move rendering up"))
                            .clicked()
                        {
                            up = Some(i);
                        }
                        if ui.button("Remove rendering").clicked() {
                            remove = Some(i);
                        }
                    });
                }
                if let Some(i) = remove {
                    draft.value.renderings.remove(i);
                } else if let Some(i) = up {
                    draft.value.renderings.swap(i, i - 1);
                }
                if ui
                    .add_enabled(
                        draft.value.renderings.len() < 256,
                        egui::Button::new("Add rendering"),
                    )
                    .clicked()
                {
                    draft.value.renderings.push(Default::default());
                }
            });
        }
        draft
            .roles
            .ui(ui, document, &mut self.expression_draft, "Activity");
        if !draft.error.is_empty() {
            ui.label(&draft.error);
        }
        if ui
            .add_enabled(
                self.expression_draft.is_none(),
                egui::Button::new("Apply roles and properties"),
            )
            .clicked()
        {
            match (
                draft.roles.build(),
                draft
                    .properties
                    .iter()
                    .map(super::event::PropertyDraft::build)
                    .collect::<Result<Vec<_>, _>>(),
            ) {
                (Ok(roles), Ok(properties)) => {
                    let mut value = draft.value.clone();
                    value.resource_roles = roles;
                    value.properties = properties;
                    if self.ctl.mutate(vec![Mutation::SetActivityProperties {
                        document_id: document.id,
                        element_id: draft.target.clone(),
                        activity: Some(value),
                    }]) {
                        self.activity_roles_draft = None;
                        self.restore_selection();
                    }
                }
                (Err(error), _) | (_, Err(error)) => draft.error = error,
            }
        }
        if ui
            .add_enabled(
                self.expression_draft.is_none(),
                egui::Button::new("Cancel roles and properties"),
            )
            .clicked()
        {
            self.activity_roles_draft = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn activity_role_apply_undo_redo_uses_shared_history() {
        let mut app = ProcessCraft::default();
        app.add(NodeKind::Task);
        let target = app.selected.clone().unwrap();
        let snapshot = app.ctl.session.snapshot();
        let document_id = snapshot.project.documents[0].id;
        let mut value = processcraft_core::bpmn_activity::ActivityProperties::default();
        value.resource_roles.push(ResourceRole {
            id: None,
            base: None,
            name: Some("Reviewer".into()),
            kind: ResourceRoleKind::HumanPerformer,
            assignment: ResourceRoleAssignment::None,
        });
        assert!(app.ctl.mutate(vec![Mutation::SetActivityProperties {
            document_id,
            element_id: target.clone(),
            activity: Some(value)
        }]));
        let role_count = |app: &ProcessCraft| {
            processcraft_core::queries::find_node(
                &app.ctl.session.snapshot().project.documents[0],
                &target,
            )
            .and_then(|node| node.activity.as_ref())
            .map_or(0, |v| v.resource_roles.len())
        };
        assert_eq!(role_count(&app), 1);
        app.history(Request::Undo);
        assert_eq!(role_count(&app), 0);
        app.history(Request::Redo);
        assert_eq!(role_count(&app), 1);
    }
    #[test]
    fn activity_role_draft_locks_ordinary_properties_and_navigation() {
        let mut app = ProcessCraft::default();
        app.add(NodeKind::Task);
        let target = app.selected.clone().unwrap();
        app.activity_roles_draft = Some(ActivityRolesDraft {
            target: target.clone(),
            value: Default::default(),
            roles: RolesDraft::default(),
            properties: vec![],
            error: String::new(),
        });
        assert!(app.ordinary_property_locked());
        assert!(!app.commit_draft());
        app.select(None);
        assert_eq!(app.selected, Some(target));
    }
    #[test]
    fn role_draft_preserves_identity_metadata_and_partial_reference() {
        let value = ResourceRole {
            id: Some("Role".parse().unwrap()),
            base: Some(Default::default()),
            name: Some(String::new()),
            kind: ResourceRoleKind::HumanPerformer,
            assignment: ResourceRoleAssignment::None,
        };
        let mut draft = RolesDraft::new(std::slice::from_ref(&value));
        assert_eq!(draft.build().unwrap(), vec![value]);
        draft.roles[0].assignment = AssignmentDraft::Resource {
            reference: Some(ReferenceDraft::Qualified(
                super::super::catalog::QNameDraft {
                    namespace: "urn:resource".into(),
                    local: "invalid name".into(),
                },
            )),
            bindings: vec![],
        };
        assert!(draft.build().is_err());
        assert!(
            matches!(&draft.roles[0].assignment,AssignmentDraft::Resource{reference:Some(ReferenceDraft::Qualified(q)),..} if q.local=="invalid name")
        );
    }
}
