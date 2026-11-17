//! `cargo run -p zlip-config --example export_schema` regenerates `zlip.schema.json`.
#![allow(clippy::print_stdout)]

fn main() {
    print!("{}", zlip_config::Config::json_schema());
}
