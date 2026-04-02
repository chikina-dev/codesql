use std::collections::{BTreeMap, HashSet};
use anyhow::Result;

use crate::catalog::Catalog;
use crate::segment::SegmentData;

pub fn compact_segments(catalog: &Catalog, through_generation: u64) -> Result<Option<SegmentData>> {
    let segment_paths = catalog.active_segment_paths(through_generation)?;
    if segment_paths.is_empty() {
        return Ok(None);
    }

    let mut compacted = SegmentData {
        generation: 0, // will be set by caller
        postings: BTreeMap::new(),
        tombstones: HashSet::new(),
    };

    for path in segment_paths {
        let segment = SegmentData::read_from_path(&path)?;

        if !segment.tombstones.is_empty() {
            compacted.postings.retain(|_, ids| {
                ids.retain(|id| !segment.tombstones.contains(id));
                !ids.is_empty()
            });
        }

        for (trigram, ids) in segment.postings {
            let compacted_ids = compacted.postings.entry(trigram).or_default();
            compacted_ids.extend(ids);
            compacted_ids.sort_unstable();
            compacted_ids.dedup();
        }
    }

    Ok(Some(compacted))
}
