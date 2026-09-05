//! World construction library, ghost progress, and independent local structures.

use std::sync::Arc;
use crate::planet::BlockPos;
use crate::registry::Registry;
use super::local_structure::{LocalStructure, LocalStructureId, RailState};
use super::multiblock::Rotation;
use super::template::{PendingFill, Template};

#[derive(Default)]
pub(super) struct Construction {
    templates: Vec<Template>,
    pending: Vec<PendingFill>,
    structures: Vec<LocalStructure>,
    next_structure_id: u64,
}

impl Construction {
    pub(super) fn templates(&self) -> &[Template] { &self.templates }
    pub(super) fn template(&self, name: &str) -> Option<&Template> { self.templates.iter().find(|template| template.name == name) }
    pub(super) fn restore_templates(&mut self, templates: Vec<Template>) { self.templates = templates; }
    pub(super) fn insert_template(&mut self, template: Template) { self.templates.push(template); }
    pub(super) fn remove_template(&mut self, name: &str) -> bool {
        let before = self.templates.len();
        self.templates.retain(|template| template.name != name);
        self.templates.len() < before
    }

    pub(super) fn install_ghost(&mut self, fill: PendingFill) {
        self.pending.retain(|prior| prior.anchor != fill.anchor);
        self.pending.push(fill);
    }
    pub(super) fn pending_at(&self, anchor: BlockPos) -> Option<&PendingFill> { self.pending.iter().find(|fill| fill.anchor == anchor) }
    pub(super) fn cancel_fill(&mut self, anchor: BlockPos) -> bool {
        let before = self.pending.len();
        self.pending.retain(|fill| fill.anchor != anchor);
        self.pending.len() < before
    }
    pub(super) fn satisfy_cell(&mut self, pos: BlockPos, name: &str) {
        self.pending.retain_mut(|fill| {
            if fill.remaining.get(&pos).is_some_and(|required| required == name) { fill.remaining.remove(&pos); }
            !fill.remaining.is_empty()
        });
    }

    pub(super) fn structures(&self) -> &[LocalStructure] { &self.structures }
    pub(super) fn structures_mut(&mut self) -> &mut [LocalStructure] { &mut self.structures }
    pub(super) fn structure(&self, id: LocalStructureId) -> Option<&LocalStructure> { self.structures.iter().find(|structure| structure.id == id) }
    pub(super) fn structure_mut(&mut self, id: LocalStructureId) -> Option<&mut LocalStructure> { self.structures.iter_mut().find(|structure| structure.id == id) }
    pub(super) fn restore_structures(&mut self, structures: Vec<LocalStructure>, next: u64) {
        self.structures = structures;
        self.next_structure_id = self.next_structure_id.max(next);
    }
    pub(super) fn remove_structure(&mut self, id: LocalStructureId) -> bool {
        let before = self.structures.len();
        self.structures.retain(|structure| structure.id != id);
        self.structures.len() < before
    }
    pub(super) fn take_structure(&mut self, index: usize) -> LocalStructure { self.structures.remove(index) }
    pub(super) fn return_structure(&mut self, index: usize, structure: LocalStructure) { self.structures.insert(index, structure); }
    pub(super) fn set_rail(&mut self, id: LocalStructureId, rail: Option<RailState>) -> bool {
        let Some(structure) = self.structure_mut(id) else { return false; };
        structure.rail = rail;
        true
    }
    pub(super) fn spawn_structure(&mut self, registry: &Arc<Registry>, template: &Template, anchor: BlockPos, rotation: Rotation) -> Result<LocalStructureId, String> {
        if template.cells.is_empty() { return Err("template has no cells".into()); }
        let mut structure = super::local_structure::from_template(template, registry);
        structure.id = LocalStructureId(self.next_structure_id);
        structure.transform.anchor = anchor;
        structure.transform.rotation = rotation;
        self.next_structure_id += 1;
        let id = structure.id;
        self.structures.push(structure);
        Ok(id)
    }
}
