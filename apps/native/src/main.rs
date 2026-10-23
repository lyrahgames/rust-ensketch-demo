fn main() {
    let args: Vec<String> = std::env::args().collect();
    let result = match args.get(1).map(String::as_str) {
        #[cfg(feature = "lua")]
        Some("--lua") => native::run_lua(args.get(2).expect("missing Lua path")),
        #[cfg(feature = "rhai")]
        Some("--rhai") => native::run_rhai(args.get(2).expect("missing Rhai path")),
        #[cfg(all(feature = "lua", feature = "rhai"))]
        Some("--both") => native::run_both(
            args.get(2).expect("missing Lua path"),
            args.get(3).expect("missing Rhai path"),
        ),
        _ => native::run_case(args.get(1).and_then(|s| s.parse().ok()).unwrap_or(1)),
    };
    if let Err(error) = result {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
