use shadow_scale_flatbuffers::shadow_scale::sim as fb;
use std::env;
use std::fs;

fn main() {
    let path = env::args().nth(1).expect("usage: dump <file>");
    let data = fs::read(path).expect("read file");
    let envelope = fb::root_as_envelope(&data).expect("parse envelope");
    println!("payload type: {:?}", envelope.payload_type());
    if let Some(snapshot) = envelope.payload_as_snapshot() {
        println!("tick: {}", snapshot.header().map(|h| h.tick()).unwrap_or(0));
        let tiles = snapshot
            .map()
            .and_then(|m| m.tiles())
            .map(|t| t.len())
            .unwrap_or(0);
        println!("tiles: {}", tiles);
        if let Some(first) = snapshot
            .map()
            .and_then(|m| m.tiles())
            .map(|tiles| tiles.get(0))
        {
            println!("first tile temp: {}", first.temperature());
        }
        let mut max_temp = i64::MIN;
        let mut min_temp = i64::MAX;
        if let Some(tiles) = snapshot.map().and_then(|m| m.tiles()) {
            for tile in tiles {
                let temp = tile.temperature();
                if temp > max_temp {
                    max_temp = temp;
                }
                if temp < min_temp {
                    min_temp = temp;
                }
            }
        }
        println!("tile temp range: {}..{}", min_temp, max_temp);
        if let Some(overlay) = snapshot.map().and_then(|m| m.terrainOverlay()) {
            println!(
                "terrain overlay: {}x{} ({} samples)",
                overlay.width(),
                overlay.height(),
                overlay.samples().map(|s| s.len()).unwrap_or_default()
            );
        }
    } else if let Some(delta) = envelope.payload_as_delta() {
        println!(
            "delta tick: {}",
            delta.header().map(|h| h.tick()).unwrap_or(0)
        );
        let mut max_temp = i64::MIN;
        let mut min_temp = i64::MAX;
        if let Some(tiles) = delta.map().and_then(|m| m.tiles()) {
            for tile in tiles {
                let temp = tile.temperature();
                max_temp = max_temp.max(temp);
                min_temp = min_temp.min(temp);
            }
        }
        println!("delta tile temp range: {}..{}", min_temp, max_temp);
        if let Some(overlay) = delta.map().and_then(|m| m.terrainOverlay()) {
            println!(
                "delta terrain overlay: {}x{} ({} samples)",
                overlay.width(),
                overlay.height(),
                overlay.samples().map(|s| s.len()).unwrap_or_default()
            );
        }
    }
}
