use super::*;

pub(crate) fn snapshot_faction_inventory(
    inventory: &FactionInventory,
) -> Vec<SchemaFactionInventoryState> {
    let mut states = Vec::new();
    for (faction, items) in inventory.iter() {
        if items.is_empty() {
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
