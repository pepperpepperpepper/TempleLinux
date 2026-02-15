#[path = "temple_hc/mod.rs"]
mod hc;

fn main() -> std::io::Result<()> {
    hc::run()
}
