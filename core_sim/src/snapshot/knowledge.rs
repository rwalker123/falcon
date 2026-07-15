use super::*;

pub(crate) fn discovery_progress_entries(
    ledger: &DiscoveryProgressLedger,
) -> Vec<DiscoveryProgressEntry> {
    let mut entries: Vec<DiscoveryProgressEntry> = Vec::new();
    for (faction_id, discoveries) in ledger.progress.iter() {
        for (discovery_id, progress) in discoveries.iter() {
            let raw = progress.raw();
            if raw <= 0 {
                continue;
            }
            entries.push(DiscoveryProgressEntry {
                faction: faction_id.0,
                discovery: *discovery_id,
                progress: raw,
            });
        }
    }
    entries.sort_unstable_by_key(|a| (a.faction, a.discovery));
    entries
}

/// Per-faction discovered-sites registry for the snapshot. Each record's `category`/`display_name`/
/// `glyph` is resolved from the sites catalog (missing entries fall back to the raw `site_id` so a
/// pruned catalog never drops a discovery). Records are emitted in a stable `(y, x, site_id)` order
/// so the snapshot is deterministic.
pub(crate) fn snapshot_discovered_sites(
    discovered: &DiscoveredSites,
    sites_config: &SitesConfigHandle,
) -> Vec<SchemaDiscoveredSitesState> {
    let cfg = sites_config.get();
    discovered
        .iter_sorted()
        .into_iter()
        .map(|(faction, records)| {
            let mut sites: Vec<SchemaDiscoveredSiteState> = records
                .iter()
                .map(|record| {
                    let def = cfg.site(&record.site_id);
                    SchemaDiscoveredSiteState {
                        x: record.pos.x,
                        y: record.pos.y,
                        site_id: record.site_id.clone(),
                        category: def.map(|d| d.category.clone()).unwrap_or_default(),
                        display_name: def
                            .map(|d| d.display_name.clone())
                            .unwrap_or_else(|| record.site_id.clone()),
                        glyph: def.map(|d| d.glyph.clone()).unwrap_or_default(),
                    }
                })
                .collect();
            sites.sort_by(|a, b| (a.y, a.x, &a.site_id).cmp(&(b.y, b.x, &b.site_id)));
            SchemaDiscoveredSitesState {
                faction: faction.0,
                sites,
            }
        })
        .collect()
}
