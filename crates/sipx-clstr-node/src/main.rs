//! The `sipx-clstr` binary.
//!
//! `run --listen <addr>` starts a node: the registrar and the forwarding core on one listener. The
//! argument surface is deliberately tiny and **provisional** — `DP-1` owns the real configuration
//! schema and replaces this rather than extending it.

use std::process::ExitCode;

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let command = args.next();
    if command.as_deref() == Some("run") {
        return run_node(&args.collect::<Vec<_>>());
    }
    match command.as_deref() {
        Some("--version" | "-V") => {
            println!(
                "sipx-clstr {} (sipx kernel {})",
                sipx_clstr_node::VERSION,
                sipx_clstr_node::KERNEL_VERSION
            );
            ExitCode::SUCCESS
        }
        None | Some("--help" | "-h") => {
            println!(
                "sipx-clstr {} — a clustered SIP proxy and registrar",
                sipx_clstr_node::VERSION
            );
            println!();
            println!(
                "No roles are implemented yet. This binary exists so that the workspace has a"
            );
            println!(
                "release target from the start; see the roadmap's M1 scope for what fills it."
            );
            println!();
            println!("  run --listen <addr>   run a node: registrar and proxy on one listener");
            println!("      --advertise <host[:port]>  what peers reach it on, if that is not the");
            println!(
                "                                 address it binds — a node behind a NAT or on a"
            );
            println!(
                "                                 private address must say so, or its Via and"
            );
            println!("                                 Record-Route name somewhere unreachable");
            println!(
                "  --version             print the version and the kernel it is built against"
            );
            ExitCode::SUCCESS
        }
        Some(other) => {
            eprintln!("sipx-clstr: unknown argument `{other}`");
            eprintln!("There is no configuration surface yet — try --help.");
            // A node that cannot do what it was asked must not look like a node that did it.
            ExitCode::from(2)
        }
    }
}

/// `sipx-clstr run --listen <addr> [--tenant <name>] [--advertise <host:port>]`
fn run_node(args: &[String]) -> ExitCode {
    let mut listen = "0.0.0.0:5060".to_owned();
    let mut tenant = "default".to_owned();
    let mut advertise: Option<String> = None;

    let mut rest = args.iter();
    while let Some(flag) = rest.next() {
        let Some(value) = rest.next() else {
            eprintln!("sipx-clstr: {flag} needs a value");
            return ExitCode::from(2);
        };
        match flag.as_str() {
            "--listen" => listen.clone_from(value),
            "--tenant" => tenant.clone_from(value),
            "--advertise" => advertise = Some(value.clone()),
            other => {
                eprintln!("sipx-clstr: unknown option `{other}`");
                return ExitCode::from(2);
            }
        }
    }

    let Ok(addr) = listen.parse() else {
        eprintln!("sipx-clstr: `{listen}` is not an address:port");
        return ExitCode::from(2);
    };

    // Bind and advertise are declared independently (`DP-5`). Without `--advertise` the node
    // advertises what it binds, which is refused for `0.0.0.0` — "everywhere" is an answer to where
    // to listen and not to where to be reached, and a node that put it in a `Record-Route` would
    // take calls that could never be transferred or hung up.
    let config = match advertise {
        Some(advertise) => sipx_clstr_node::driver::NodeConfig::advertising(addr, &advertise),
        None => sipx_clstr_node::driver::NodeConfig::new(addr),
    };
    let mut config = match config {
        Ok(config) => config,
        Err(error) => {
            eprintln!("sipx-clstr: {error}");
            eprintln!("Pass --advertise <host[:port]> with the address peers reach this node on.");
            return ExitCode::from(2);
        }
    };
    config.tenant = tenant;

    let Ok(runtime) = tokio::runtime::Runtime::new() else {
        eprintln!("sipx-clstr: could not start the async runtime");
        return ExitCode::FAILURE;
    };

    match runtime.block_on(async {
        tracing_subscriber::fmt()
            .with_env_filter(
                tracing_subscriber::EnvFilter::try_from_default_env()
                    .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
            )
            .with_writer(std::io::stderr)
            // No colour: this log is read by scripts as often as by people, and escape codes
            // between a field name and its value defeat an honest `grep`.
            .with_ansi(false)
            .try_init()
            .ok();
        // The driver announces the address once it is actually bound.
        sipx_clstr_node::driver::run(config).await
    }) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("sipx-clstr: {error}");
            ExitCode::FAILURE
        }
    }
}
