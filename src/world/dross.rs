//! Authoritative bridge between sparse dross transaction owners and the
//! dense, weather-coupled planetary subledger.





const MAX_DROSS_IMPORT_ACCOUNTS: usize = 12;

#[derive(Clone)]
struct DenseImportBefore {
    cell: crate::arcane_geography::ArcaneDynamicCell,
    carrier: crate::dross::DrossCellState,
    provenance: Option<crate::dross::DrossProvenance>,
}

struct ScarManifestCandidate {
    score: u64,
    region: crate::planet_atlas::AtlasPos,
    index: usize,
    carrier: crate::dross::DrossCarrier,
    available: u64,
    kind: crate::dross::ScarKind,
    content_id: String,
    site_slot: u8,
}


mod exposure;
mod ambient_import;
mod tick;
mod manifestation;
mod scar_staging;
mod excavation;
mod release;

