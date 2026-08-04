//! Selection tools: box select, lasso select, batch operations

use crate::command::*;
use slg_core::map::grid::HexCoord;
use slg_data::ids::*;
use slg_data::map_doc::*;
use std::collections::BTreeSet;

// ---------------------------------------------------------------------------
// SelectionRegion
// ---------------------------------------------------------------------------

/// A region of selected hex tiles, stored as a BTreeSet of TileKeys.
#[derive(Debug, Clone)]
pub struct SelectionRegion {
    pub tiles: BTreeSet<TileKey>,
    pub bounds_min: HexCoord,
    pub bounds_max: HexCoord,
}

impl SelectionRegion {
    pub fn new() -> Self {
        Self {
            tiles: BTreeSet::new(),
            bounds_min: HexCoord::new(i32::MAX, i32::MAX),
            bounds_max: HexCoord::new(i32::MIN, i32::MIN),
        }
    }

    pub fn insert(&mut self, coord: HexCoord) {
        let key = coord.to_tile_key();
        self.tiles.insert(key);
        self.bounds_min = HexCoord::new(
            self.bounds_min.q.min(coord.q),
            self.bounds_min.r.min(coord.r),
        );
        self.bounds_max = HexCoord::new(
            self.bounds_max.q.max(coord.q),
            self.bounds_max.r.max(coord.r),
        );
    }

    pub fn contains(&self, coord: &HexCoord) -> bool {
        self.tiles.contains(&coord.to_tile_key())
    }

    pub fn is_empty(&self) -> bool {
        self.tiles.is_empty()
    }

    pub fn len(&self) -> usize {
        self.tiles.len()
    }
}

impl Default for SelectionRegion {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// BoxSelect
// ---------------------------------------------------------------------------

/// Box selection: selects all hex tiles within the rectangular region
/// defined by two opposite corners.
pub struct BoxSelect {
    pub start: HexCoord,
    pub end: HexCoord,
    pub selection: SelectionRegion,
}

impl BoxSelect {
    pub fn new(start: HexCoord, end: HexCoord) -> Self {
        let mut selection = SelectionRegion::new();

        let min_q = start.q.min(end.q);
        let max_q = start.q.max(end.q);
        let min_r = start.r.min(end.r);
        let max_r = start.r.max(end.r);

        for q in min_q..=max_q {
            for r in min_r..=max_r {
                selection.insert(HexCoord::new(q, r));
            }
        }

        Self {
            start,
            end,
            selection,
        }
    }
}

// ---------------------------------------------------------------------------
// LassoSelect
// ---------------------------------------------------------------------------

/// Lasso selection: selects a set of specific hex coordinates.
///
/// Simplified implementation: stores the given points directly.
/// A full implementation would use polygon fill on the hex grid.
pub struct LassoSelect {
    pub points: Vec<HexCoord>,
    pub selection: SelectionRegion,
}

impl LassoSelect {
    pub fn new(points: Vec<HexCoord>) -> Self {
        let mut selection = SelectionRegion::new();

        for point in &points {
            selection.insert(*point);
        }

        Self { points, selection }
    }
}

// ---------------------------------------------------------------------------
// BatchPaint
// ---------------------------------------------------------------------------

/// Batch terrain paint: change terrain type for all tiles in a selection.
pub struct BatchPaint {
    pub selection: SelectionRegion,
    pub new_terrain: TerrainTypeId,
    pub old_terrains: Vec<(TileKey, TerrainTypeId)>,
}

impl EditorCommand for BatchPaint {
    fn execute(&self, _doc: &mut MapDocument) -> Result<(), String> {
        // Simplified: would iterate selection.tiles, read old terrain at each,
        // store in old_terrains, then write new_terrain.
        // Requires mutable old_terrains; deferred to full implementation.
        Ok(())
    }

    fn undo(&self, _doc: &mut MapDocument) -> Result<(), String> {
        // Restore old_terrains for each tile in the selection.
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// BatchSetOwner
// ---------------------------------------------------------------------------

/// Batch set owner: change faction ownership for all tiles in a selection.
pub struct BatchSetOwner {
    pub selection: SelectionRegion,
    pub new_owner: Option<FactionId>,
    pub old_owners: Vec<(TileKey, Option<FactionId>)>,
}

impl EditorCommand for BatchSetOwner {
    fn execute(&self, _doc: &mut MapDocument) -> Result<(), String> {
        // Simplified: would iterate selection.tiles, save old owner, set new_owner.
        Ok(())
    }

    fn undo(&self, _doc: &mut MapDocument) -> Result<(), String> {
        // Restore old_owners for each tile in the selection.
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// CopyRegion / PasteRegion
// ---------------------------------------------------------------------------

/// Copy a selected region of the map into a clipboard (a full MapDocument clone).
pub struct CopyRegion {
    pub source: SelectionRegion,
}

impl CopyRegion {
    pub fn execute(&self, doc: &MapDocument) -> MapDocument {
        // Simplified: return a full clone of the document.
        // A full implementation would extract only the selected tiles.
        doc.clone()
    }
}

/// Paste a clipboard document at a target offset.
pub struct PasteRegion {
    pub target: HexCoord,
    pub clipboard: MapDocument,
}

impl EditorCommand for PasteRegion {
    fn execute(&self, _doc: &mut MapDocument) -> Result<(), String> {
        // Simplified: would overlay clipboard tiles onto doc at target offset.
        Ok(())
    }

    fn undo(&self, _doc: &mut MapDocument) -> Result<(), String> {
        // Restore original tiles at the paste region.
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_box_select() {
        let select = BoxSelect::new(HexCoord::new(0, 0), HexCoord::new(2, 2));
        // 3x3 = 9 tiles
        assert_eq!(select.selection.len(), 9);
        assert!(select.selection.contains(&HexCoord::new(1, 1)));
        assert!(!select.selection.contains(&HexCoord::new(3, 3)));
    }

    #[test]
    fn test_box_select_single_tile() {
        let select = BoxSelect::new(HexCoord::new(5, 5), HexCoord::new(5, 5));
        assert_eq!(select.selection.len(), 1);
        assert!(select.selection.contains(&HexCoord::new(5, 5)));
    }

    #[test]
    fn test_box_select_reversed_corners() {
        // start > end should still work (min/max handles it)
        let select = BoxSelect::new(HexCoord::new(2, 2), HexCoord::new(0, 0));
        assert_eq!(select.selection.len(), 9);
        assert!(select.selection.contains(&HexCoord::new(0, 0)));
        assert!(select.selection.contains(&HexCoord::new(2, 2)));
    }

    #[test]
    fn test_selection_region() {
        let mut region = SelectionRegion::new();
        assert!(region.is_empty());

        region.insert(HexCoord::new(5, 5));
        region.insert(HexCoord::new(6, 5));

        assert_eq!(region.len(), 2);
        assert!(region.contains(&HexCoord::new(5, 5)));
        assert!(region.contains(&HexCoord::new(6, 5)));
        assert!(!region.contains(&HexCoord::new(7, 5)));
    }

    #[test]
    fn test_selection_region_bounds() {
        let mut region = SelectionRegion::new();
        region.insert(HexCoord::new(3, 7));
        region.insert(HexCoord::new(10, 2));

        assert_eq!(region.bounds_min, HexCoord::new(3, 2));
        assert_eq!(region.bounds_max, HexCoord::new(10, 7));
    }

    #[test]
    fn test_selection_region_dedup() {
        let mut region = SelectionRegion::new();
        region.insert(HexCoord::new(1, 1));
        region.insert(HexCoord::new(1, 1)); // duplicate
        assert_eq!(region.len(), 1);
    }

    #[test]
    fn test_lasso_select() {
        let points = vec![
            HexCoord::new(0, 0),
            HexCoord::new(1, 0),
            HexCoord::new(1, 1),
            HexCoord::new(0, 1),
        ];
        let select = LassoSelect::new(points);
        assert_eq!(select.selection.len(), 4);
    }

    #[test]
    fn test_lasso_select_empty() {
        let select = LassoSelect::new(vec![]);
        assert!(select.selection.is_empty());
    }

    #[test]
    fn test_batch_paint_creation() {
        let mut selection = SelectionRegion::new();
        selection.insert(HexCoord::new(0, 0));
        selection.insert(HexCoord::new(1, 0));

        let cmd = BatchPaint {
            selection,
            new_terrain: "terrain_forest".to_string(),
            old_terrains: Vec::new(),
        };

        assert_eq!(cmd.selection.len(), 2);
        assert_eq!(cmd.new_terrain, "terrain_forest");
    }

    #[test]
    fn test_batch_set_owner_creation() {
        let mut selection = SelectionRegion::new();
        selection.insert(HexCoord::new(3, 3));

        let cmd = BatchSetOwner {
            selection,
            new_owner: Some("faction_red".to_string()),
            old_owners: Vec::new(),
        };

        assert_eq!(cmd.selection.len(), 1);
        assert_eq!(cmd.new_owner, Some("faction_red".to_string()));
    }

    #[test]
    fn test_copy_region() {
        let doc = create_test_doc();
        let mut selection = SelectionRegion::new();
        selection.insert(HexCoord::new(0, 0));

        let copy = CopyRegion { source: selection };
        let clipboard = copy.execute(&doc);
        assert_eq!(clipboard.meta.name, doc.meta.name);
    }

    #[test]
    fn test_paste_region_creation() {
        let doc = create_test_doc();
        let cmd = PasteRegion {
            target: HexCoord::new(10, 10),
            clipboard: doc,
        };
        assert_eq!(cmd.target, HexCoord::new(10, 10));
    }

    #[test]
    fn test_box_select_with_command_history() {
        use crate::command::CommandHistory;

        let mut doc = create_test_doc();
        let mut history = CommandHistory::new(200);

        let mut selection = SelectionRegion::new();
        selection.insert(HexCoord::new(1, 1));

        let cmd = BatchPaint {
            selection,
            new_terrain: "terrain_forest".to_string(),
            old_terrains: Vec::new(),
        };

        // Execute and undo should not panic
        history.execute(Box::new(cmd), &mut doc).unwrap();
        history.undo(&mut doc).unwrap();
        history.redo(&mut doc).unwrap();
    }

    fn create_test_doc() -> MapDocument {
        use std::collections::BTreeMap;
        MapDocument {
            meta: MapMeta {
                name: "test".to_string(),
                seed: 42,
                width: 32,
                height: 32,
                preset_name: None,
            },
            terrain: TerrainLayer {
                rle_data: vec![("terrain_plains".to_string(), 1024)],
                total_tiles: 1024,
            },
            resources: ResourceLayer {
                entries: BTreeMap::new(),
            },
            entities: EntityLayer {
                placements: BTreeMap::new(),
            },
            rules: RuleLayer {
                zones: vec![],
                triggers: vec![],
            },
            rivers: Default::default(),
        }
    }
}
