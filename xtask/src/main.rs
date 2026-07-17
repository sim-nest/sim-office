#![forbid(unsafe_code)]

#[cfg(test)]
mod ledger_pins;
mod simdoc;

fn main() {
    if let Err(err) = simdoc::run(std::env::args().collect()) {
        eprintln!("{err}");
        std::process::exit(1);
    }
}
