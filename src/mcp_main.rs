use phonetik::{mcp, Phonetik};

fn main() {
    let engine = Phonetik::new();
    mcp::run_stdio(engine);
}
