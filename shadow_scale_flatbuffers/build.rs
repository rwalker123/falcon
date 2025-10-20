use std::fs;
use std::path::Path;

fn main() {
    let schema = Path::new("../sim_schema/schemas/snapshot.fbs");
    let out_dir = Path::new("src/generated");
    fs::create_dir_all(out_dir).expect("failed to create generated dir");
    flatc_rust::run(flatc_rust::Args {
        inputs: &[schema],
        out_dir,
        ..Default::default()
    })
    .expect("Failed to run flatc");
}
