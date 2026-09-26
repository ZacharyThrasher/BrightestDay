//! Cybertron preview (feature `cybertron`) — placeholder, replaced below.
fn main() {
    let (w, info) = bd_voxel::maps::cybertron::generate_cybertron(1);
    println!("{} solid voxels, {} spawns", w.solid_count(), info.spawns.len());
}
