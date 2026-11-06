//! **The seat plays by the player's rules on a chief's row** (`docs/plan_labor_at_scale.md` ->
//! "Band orders - the chief"). A band with a leader has a chief who works the rows its orders list;
//! the sim refuses `assign_labor`, `stop_labor` and `work_priority` on such a row until the player
//! crews it ("Crew it yourself", `band_order … release <row>`). The seat has no UI, so it says the
//! same thing on the wire: [`release_chief_rows`] runs over the turn's accepted commands and puts a
//! one-row release in front of the first hand command on every row the frame shows as `chiefOwned`.
//!
//! ⛔ **Per ROW, never per kind, and never a claim back.** The release names the one stand the
//! seat is about to work, so the chief keeps every other row of the order; the seat never hands a
//! row back. A row that is not `chiefOwned` (a new patch, a row already released) gets no release,
//! so the commands the specialists emit are untouched there.
//!
//! The pass sits after arbitration and reads only the frame: a specialist never proposes a release,
//! so there is no second proposal for a release to conflict with. [`Cost::claimed`] still claims
//! the row a `BandOrder` row-command names, under the key the labor commands on it claim.
//!
//! [`Cost::claimed`]: crate::specialists::Cost::claimed

use sim_runtime::{BandOrderAction, BandOrderRow, CommandPayload};

use crate::specialists::food::{ROLE_FORAGE, ROLE_HUNT};
use crate::specialists::ROLE_EXTRACT;
use crate::view::SeatView;

/// Which source a hand command names: the same address the wire uses for it.
struct Address<'a> {
    band_id: Option<u64>,
    role: &'static str,
    x: u32,
    y: u32,
    herd: &'a str,
    /// The material a dig stand is named by, when the command says.
    material: Option<&'a str>,
}

/// The source a hand labor command names, or `None` for a command that names no chief-run row
/// (a builders pool, roadwork, a tile improvement's branch, any other verb).
fn address_of(command: &CommandPayload) -> Option<Address<'_>> {
    match command {
        CommandPayload::AssignLabor {
            band_id,
            role,
            target_x,
            target_y,
            fauna_id,
            species,
            ..
        } => {
            let role = match role.to_ascii_lowercase().as_str() {
                ROLE_FORAGE => ROLE_FORAGE,
                ROLE_HUNT => ROLE_HUNT,
                ROLE_EXTRACT => ROLE_EXTRACT,
                _ => return None,
            };
            Some(Address {
                band_id: *band_id,
                role,
                x: target_x.unwrap_or_default(),
                y: target_y.unwrap_or_default(),
                herd: fauna_id.as_deref().unwrap_or_default(),
                material: (role == ROLE_EXTRACT)
                    .then_some(species.as_deref())
                    .flatten(),
            })
        }
        CommandPayload::StopLabor {
            band_id,
            target_x,
            target_y,
            herd_id,
            material,
            branch: None,
            roadwork: false,
            ..
        }
        | CommandPayload::WorkPriority {
            band_id,
            target_x,
            target_y,
            herd_id,
            material,
            branch: None,
            ..
        } => Some(Address {
            band_id: Some(*band_id),
            role: if herd_id.is_some() {
                ROLE_HUNT
            } else if material.is_some() {
                ROLE_EXTRACT
            } else {
                ROLE_FORAGE
            },
            x: target_x.unwrap_or_default(),
            y: target_y.unwrap_or_default(),
            herd: herd_id.as_deref().unwrap_or_default(),
            material: material.as_deref(),
        }),
        _ => None,
    }
}

/// The `band_order … release <row>` that hands the chief's row under `command` to the seat, or
/// `None` when the command names no row the chief works.
fn release_for(view: &SeatView, faction: u32, command: &CommandPayload) -> Option<CommandPayload> {
    let address = address_of(command)?;
    let mut own = view.own_bands(faction);
    let band = match address.band_id {
        Some(id) => view.band(id).filter(|band| band.faction == faction)?,
        // A command with no band is the seat's only band's.
        None => {
            let only = own.next()?;
            own.next().is_none().then_some(only)?
        }
    };
    let row = band.labor_assignments.iter().find(|row| {
        row.chief_owned
            && row.kind == address.role
            && if address.role == ROLE_HUNT {
                row.fauna_id == address.herd
            } else {
                row.target_x == address.x && row.target_y == address.y
            }
            && address
                .material
                .is_none_or(|material| row.order_kind == material)
    })?;
    Some(CommandPayload::BandOrder {
        faction_id: faction,
        band_id: band.band_id,
        kind: row.order_kind.clone(),
        action: BandOrderAction::Release,
        row: Some(if address.role == ROLE_HUNT {
            BandOrderRow::Herd {
                id: row.fauna_id.clone(),
            }
        } else {
            BandOrderRow::Tile {
                x: row.target_x,
                y: row.target_y,
            }
        }),
    })
}

/// **`commands` with a one-row release in front of the first hand command on each chief-run row.**
/// Order is kept: every other command stays where it was, and a row two commands name is released
/// once.
pub fn release_chief_rows(
    view: &SeatView,
    faction: u32,
    commands: Vec<CommandPayload>,
) -> Vec<CommandPayload> {
    let mut released: Vec<CommandPayload> = Vec::new();
    let mut out = Vec::with_capacity(commands.len());
    for command in commands {
        if let Some(release) = release_for(view, faction, &command) {
            if !released.contains(&release) {
                released.push(release.clone());
                out.push(release);
            }
        }
        out.push(command);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use sim_runtime::{LaborAssignmentState, PopulationCohortState, WorldSnapshot};

    use crate::specialists::Cost;

    const FACTION: u32 = 1;
    const BAND: u64 = 2;
    const PATCH: (u32, u32) = (3, 11);
    const HERD: &str = "herd_marsh_grazer_04";
    const STAND: (u32, u32) = (5, 6);

    fn row(kind: &str, x: u32, y: u32, herd: &str, order_kind: &str) -> LaborAssignmentState {
        LaborAssignmentState {
            kind: kind.into(),
            target_x: x,
            target_y: y,
            fauna_id: herd.into(),
            workers: 2,
            order_kind: order_kind.into(),
            chief_owned: !order_kind.is_empty(),
            ..Default::default()
        }
    }

    fn a_view(rows: Vec<LaborAssignmentState>) -> SeatView {
        let mut snapshot = WorldSnapshot::default();
        snapshot.populations.push(PopulationCohortState {
            faction: FACTION,
            band_id: BAND,
            labor_assignments: rows,
            ..Default::default()
        });
        SeatView {
            snapshot,
            last_acted_tick: None,
        }
    }

    fn assign(role: &str, x: u32, y: u32, herd: Option<&str>) -> CommandPayload {
        CommandPayload::AssignLabor {
            faction_id: FACTION,
            band_id: Some(BAND),
            role: role.into(),
            workers: 1,
            target_x: herd.is_none().then_some(x),
            target_y: herd.is_none().then_some(y),
            fauna_id: herd.map(str::to_owned),
            policy: None,
            species: None,
            floor: None,
            kit_id: None,
            take_species: Vec::new(),
            move_with_herd: false,
        }
    }

    fn stop(x: u32, y: u32, material: Option<&str>) -> CommandPayload {
        CommandPayload::StopLabor {
            faction_id: FACTION,
            band_id: BAND,
            target_x: Some(x),
            target_y: Some(y),
            herd_id: None,
            material: material.map(str::to_owned),
            branch: None,
            roadwork: false,
        }
    }

    fn release(kind: &str, row: BandOrderRow) -> CommandPayload {
        CommandPayload::BandOrder {
            faction_id: FACTION,
            band_id: BAND,
            kind: kind.into(),
            action: BandOrderAction::Release,
            row: Some(row),
        }
    }

    /// A chief-owned food row: the release comes first, names the one row, and the assign follows.
    #[test]
    fn a_chief_owned_food_row_is_released_before_the_assign() {
        let view = a_view(vec![
            row("forage", PATCH.0, PATCH.1, "", "food"),
            row("hunt", 0, 0, HERD, "food"),
        ]);
        let forage = assign("forage", PATCH.0, PATCH.1, None);
        let hunt = assign("hunt", 0, 0, Some(HERD));
        let sent = release_chief_rows(&view, FACTION, vec![forage.clone(), hunt.clone()]);
        assert_eq!(
            sent,
            vec![
                release(
                    "food",
                    BandOrderRow::Tile {
                        x: PATCH.0,
                        y: PATCH.1
                    }
                ),
                forage,
                release("food", BandOrderRow::Herd { id: HERD.into() }),
                hunt,
            ]
        );
    }

    /// A row the chief does not run gets no release, and a row two commands name is released once.
    #[test]
    fn only_a_chief_run_row_is_released_and_only_once() {
        let view = a_view(vec![
            row("forage", PATCH.0, PATCH.1, "", "food"),
            row("forage", 9, 9, "", ""),
        ]);
        let plain = vec![assign("forage", 9, 9, None), assign("forage", 1, 1, None)];
        assert_eq!(release_chief_rows(&view, FACTION, plain.clone()), plain);
        let sent = release_chief_rows(
            &view,
            FACTION,
            vec![
                assign("forage", PATCH.0, PATCH.1, None),
                stop(PATCH.0, PATCH.1, None),
            ],
        );
        assert_eq!(sent.len(), 3, "one release, then both commands: {sent:?}");
        assert!(matches!(
            sent[0],
            CommandPayload::BandOrder {
                action: BandOrderAction::Release,
                ..
            }
        ));
    }

    /// A dig stand is released under its material, by tile; the labor command that follows claims
    /// the row, so arbitration sees one row.
    #[test]
    fn a_dig_stand_is_released_by_material() {
        let view = a_view(vec![row("extract", STAND.0, STAND.1, "", "stone")]);
        let sent = release_chief_rows(&view, FACTION, vec![stop(STAND.0, STAND.1, Some("stone"))]);
        assert_eq!(
            sent[0],
            release(
                "stone",
                BandOrderRow::Tile {
                    x: STAND.0,
                    y: STAND.1
                }
            )
        );
        assert!(!Cost::claimed(0, BAND, &sent[1..]).rows.is_empty());
        // The release claims the very row the labor command after it does.
        assert_eq!(
            Cost::claimed(0, BAND, &sent[..1]).rows,
            Cost::claimed(0, BAND, &sent[1..]).rows
        );
    }

    /// The release renders to the text the sim parses back.
    #[test]
    fn the_release_renders_as_the_wire_line() {
        let tile = release(
            "food",
            BandOrderRow::Tile {
                x: PATCH.0,
                y: PATCH.1,
            },
        );
        let herd = release("food", BandOrderRow::Herd { id: HERD.into() });
        for (command, line) in [
            (tile, "band_order 1 2 food release 3 11".to_owned()),
            (herd, format!("band_order 1 2 food release {HERD}")),
        ] {
            assert_eq!(sim_runtime::render_command_line(&command), line);
            assert_eq!(sim_runtime::parse_command_line(&line).unwrap(), command);
        }
    }
}
