#[cfg(target_os = "macos")]
mod macos;

#[cfg(target_os = "macos")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    macos::main()
}

#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("kasane-metal benchmark requires macOS");
    std::process::exit(1);
}
