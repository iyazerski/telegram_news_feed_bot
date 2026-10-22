// The migrate! macro reads migrations/ at compile time, so a new or changed migration must rebuild the binary.
fn main() {
    println!("cargo:rerun-if-changed=../migrations");
}
