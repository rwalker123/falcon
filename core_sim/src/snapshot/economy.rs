use super::*;

/// **THE VIEWER'S OWN STOCKPILE, AND NOBODY ELSE'S.**
///
/// A stockpile is a number in a ledger with no thing on the map to look at, so there is no
/// "redacted" tier for it the way a band has one (`factions.md` → "Which frame sections are
/// viewer-scoped"): either it is yours and the frame states it, or it is not and the frame is silent.
/// Before this every client received every faction's whole stockpile, item by item.
pub(crate) fn snapshot_faction_inventory(
    inventory: &FactionInventory,
    viewer: FactionId,
) -> Vec<SchemaFactionInventoryState> {
    let mut states = Vec::new();
    for (faction, items) in inventory.iter() {
        if *faction != viewer || items.is_empty() {
            continue;
        }
        let mut entries: Vec<_> = items
            .iter()
            .map(|(item, quantity)| SchemaFactionInventoryEntryState {
                item: item.clone(),
                quantity: *quantity,
            })
            .collect();
        entries.sort_by(|a, b| a.item.cmp(&b.item));
        states.push(SchemaFactionInventoryState {
            faction: faction.0,
            inventory: entries,
        });
    }
    states.sort_by_key(|a| a.faction);
    states
}
