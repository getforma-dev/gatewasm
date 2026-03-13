fn main() {
    let dist = std::path::Path::new("../../admin/dist");
    // Watch every file in dist, not just the directory entry
    if dist.exists() {
        for entry in std::fs::read_dir(dist).unwrap() {
            let path = entry.unwrap().path();
            println!("cargo:rerun-if-changed={}", path.display());
        }
    }
    println!("cargo:rerun-if-changed={}", dist.display());
}
