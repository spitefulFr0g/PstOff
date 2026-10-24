#[cfg(windows)]
fn main() {
    eprintln!("PstOff Windows GUI is not implemented yet.");
    std::process::exit(1);
}
#[cfg(not(windows))]
fn main() {
    eprintln!("PstOff GUI requires Windows.");
    std::process::exit(1);
}
