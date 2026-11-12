use super::catalog::{
    IdentityDraft, id_build, id_selector, id_string, optional_bool, optional_text,
};
use super::data::{
    ItemDraft, PortDraft, SetDraft, build_item, build_sets, item_ui, set_drafts, sets_ui,
};
use processcraft_core::bpmn_data::{DataPort, IoSpecification};
#[derive(Clone)]
pub(super) struct CallableIoDraft {
    present: bool,
    original: Option<IoSpecification>,
    id: IdentityDraft,
    inputs: Vec<PortDraft>,
    outputs: Vec<PortDraft>,
    input_sets: Vec<SetDraft>,
    output_sets: Vec<SetDraft>,
}
impl CallableIoDraft {
    pub(super) fn new(value: &Option<IoSpecification>) -> Self {
        let ports = |values: &[DataPort]| {
            values
                .iter()
                .map(|v| PortDraft {
                    value: v.clone(),
                    id: id_string(&v.id),
                    item: v.item_aware.as_ref().map(ItemDraft::from_item),
                })
                .collect()
        };
        Self {
            present: value.is_some(),
            original: value.clone(),
            id: value
                .as_ref()
                .map_or_else(Default::default, |v| id_string(&v.id)),
            inputs: value.as_ref().map_or_else(Vec::new, |v| ports(&v.inputs)),
            outputs: value.as_ref().map_or_else(Vec::new, |v| ports(&v.outputs)),
            input_sets: value
                .as_ref()
                .map_or_else(Vec::new, |v| set_drafts(&v.input_sets)),
            output_sets: value
                .as_ref()
                .map_or_else(Vec::new, |v| set_drafts(&v.output_sets)),
        }
    }
    pub(super) fn build(&self) -> Result<Option<IoSpecification>, String> {
        if !self.present {
            return Ok(None);
        }
        let mut io = self.original.clone().unwrap_or_default();
        io.id = id_build(&self.id)?;
        let ports = |values: &[PortDraft]| {
            values
                .iter()
                .map(|p| {
                    let mut v = p.value.clone();
                    v.id = id_build(&p.id)?;
                    v.item_aware = build_item(&p.item)?;
                    Ok(v)
                })
                .collect::<Result<Vec<_>, String>>()
        };
        io.inputs = ports(&self.inputs)?;
        io.outputs = ports(&self.outputs)?;
        io.input_sets = build_sets(&self.input_sets)?;
        io.output_sets = build_sets(&self.output_sets)?;
        Ok(Some(io))
    }
    pub(super) fn ui(&mut self, ui: &mut egui::Ui) {
        ui.checkbox(&mut self.present, "I/O specification present");
        if !self.present {
            return;
        }
        optional_text(ui, "I/O specification ID", &mut self.id.value);
        for (label, ports) in [("Inputs", &mut self.inputs), ("Outputs", &mut self.outputs)] {
            ui.collapsing(label, |ui| {
                let mut remove = None;
                let mut up = None;
                for (i, p) in ports.iter_mut().enumerate() {
                    ui.push_id(i, |ui| {
                        ui.label(format!("Port {}", i + 1));
                        optional_text(ui, "ID", &mut p.id.value);
                        optional_text(ui, "Name", &mut p.value.name);
                        optional_bool(ui, "Collection", &mut p.value.is_collection);
                        item_ui(ui, &mut p.item);
                        if ui
                            .add_enabled(i > 0, egui::Button::new("Move port up"))
                            .clicked()
                        {
                            up = Some(i);
                        }
                        if ui.button("Remove port").clicked() {
                            remove = Some(i);
                        }
                    });
                }
                if let Some(i) = remove {
                    ports.remove(i);
                } else if let Some(i) = up {
                    ports.swap(i, i - 1);
                }
                if ui
                    .add_enabled(ports.len() < 256, egui::Button::new("Add port"))
                    .clicked()
                {
                    ports.push(PortDraft {
                        value: DataPort {
                            id: None,
                            base: None,
                            name: None,
                            item_aware: None,
                            is_collection: None,
                        },
                        id: IdentityDraft::default(),
                        item: None,
                    });
                }
            });
        }
        let inputs = self
            .inputs
            .iter()
            .filter_map(|p| id_selector(&p.id))
            .collect::<Vec<_>>();
        let outputs = self
            .outputs
            .iter()
            .filter_map(|p| id_selector(&p.id))
            .collect::<Vec<_>>();
        let input_sets = self
            .input_sets
            .iter()
            .filter_map(|s| id_selector(&s.id))
            .collect::<Vec<_>>();
        let output_sets = self
            .output_sets
            .iter()
            .filter_map(|s| id_selector(&s.id))
            .collect::<Vec<_>>();
        sets_ui(
            ui,
            "Input sets",
            &mut self.input_sets,
            &inputs,
            &output_sets,
        );
        sets_ui(
            ui,
            "Output sets",
            &mut self.output_sets,
            &outputs,
            &input_sets,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn io_draft_keeps_anonymous_ids_empty_names_and_metadata() {
        let io = Some(IoSpecification {
            id: None,
            base: Some(Default::default()),
            inputs: vec![DataPort {
                id: None,
                base: Some(Default::default()),
                name: Some(String::new()),
                item_aware: None,
                is_collection: Some(false),
            }],
            outputs: vec![],
            input_sets: vec![],
            output_sets: vec![],
        });
        let mut draft = CallableIoDraft::new(&io);
        assert_eq!(draft.build().unwrap(), io);
        draft.inputs[0].id.value = Some("bad name".into());
        assert!(draft.build().is_err());
        assert_eq!(draft.inputs[0].id.value.as_deref(), Some("bad name"));
    }
}
