//! The `sipx-clstr` binary.
//!
//! `run --config <path>` starts a node: it reads the cluster configuration document, projects it
//! through the identity this node was given from outside, and runs what comes out. The document is
//! the configuration surface — `DP-1` specified it and `DP-8` reads it — and it **replaced** the three
//! provisional flags rather than being added beside them, because two configuration surfaces is the
//! thing the schema exists to remove.
//!
//! The arguments are parsed by `clap`. They used to be parsed by hand, on the stated grounds that the
//! surface was "deliberately tiny and provisional"; that stopped being true when it grew a node
//! identity, and a hand-rolled parser whose justification has expired is just a parser with fewer
//! tests than the one in the registry.

use std::process::ExitCode;

use clap::{Parser, Subcommand};
use sipx_clstr_node::startup::{IdentityArgs, StartupError, environment, from_document};

#[derive(Debug, Parser)]
#[command(
    name = "sipx-clstr",
    version = concat!(env!("CARGO_PKG_VERSION")),
    about = "A clustered SIP proxy and registrar",
    long_about = "One node registers users and proxies calls between them. Several nodes sharing a \
                  location store share their registrations. Affinity tokens, trunks and media \
                  control are specified but not implemented; see the documentation.",
    disable_version_flag = true
)]
struct Cli {
    /// Print the version and the sipx kernel it is built against
    #[arg(short = 'V', long, global = true)]
    version: bool,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Run a node from a cluster configuration document
    ///
    /// Identity comes from outside the document, because the document is the same on every node.
    /// Each of `--node`, `--zone` and `--roles` may instead be given as `SIPX_CLSTR_NODE`,
    /// `SIPX_CLSTR_ZONE` or `SIPX_CLSTR_ROLES`, which is how a Kubernetes manifest supplies them
    /// without a shell wrapper.
    ///
    /// Secrets are named in the document by reference and resolved from the environment:
    /// `dsnRef: location-dsn` reads `LOCATION_DSN`. A reference that does not resolve stops the node
    /// rather than being ignored.
    Run {
        /// The cluster configuration document: YAML, JSON or TOML
        #[arg(long, value_name = "PATH")]
        config: String,

        /// This node's logical id. Never read from the document, because the document is the same on
        /// every node
        #[arg(long, value_name = "1..65535")]
        node: Option<u16>,

        /// This node's failure domain
        #[arg(long, value_name = "NAME")]
        zone: Option<String>,

        /// What this node runs: edge, registrar, inbound-proxy, outbound-proxy, e2e-tester, echo.
        /// `echo` and `e2e-tester` are refused beside any proxy role
        #[arg(long, value_name = "A,B")]
        roles: Option<String>,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();

    if cli.version {
        println!(
            "sipx-clstr {} (sipx kernel {})",
            sipx_clstr_node::VERSION,
            sipx_clstr_node::KERNEL_VERSION
        );
        return ExitCode::SUCCESS;
    }

    match cli.command {
        Some(Command::Run {
            config,
            node,
            zone,
            roles,
        }) => run_node(&config, IdentityArgs { node, zone, roles }),
        None => {
            // `clap` prints its own help for `--help`; a bare invocation gets it too rather than a
            // usage error, because "what is this" is a reasonable thing to ask a binary.
            let mut command = <Cli as clap::CommandFactory>::command();
            let _ = command.print_help();
            ExitCode::SUCCESS
        }
    }
}

fn run_node(path: &str, identity: IdentityArgs) -> ExitCode {
    // Installed **before** the document is read, not after. It used to be the other way round, and
    // the consequence was not subtle: the loader's "this build does not apply that" warning was
    // emitted into a process with no subscriber, so a release shipped four silently-discarded
    // security keys with the detector already written and correct.
    //
    // Refusals stay on `eprintln!` regardless, so a document that cannot be read still reports even
    // if the subscriber could not be installed at all.
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with_writer(std::io::stderr)
        // No colour: this log is read by scripts as often as by people, and escape codes between a
        // field name and its value defeat an honest `grep`.
        .with_ansi(false)
        .try_init()
        .ok();

    let env = environment();

    // The environment fallback lives in `or_env` rather than in a `clap(env = …)` attribute, so
    // there is one tested place that decides how a flag and a variable combine, instead of that rule
    // being split between the parser and the resolver.
    let identity = match identity.or_env(&env).resolve() {
        Ok(identity) => identity,
        Err(error) => {
            eprintln!("sipx-clstr: {error}");
            return ExitCode::from(2);
        }
    };

    let config = match from_document(path, &identity, &env) {
        Ok(config) => config,
        Err(StartupError::Rejected(problems)) => {
            // Every problem, in the order the loader established. Printing only the first would
            // waste the property that makes a five-mistake document cost one restart, not five.
            eprintln!(
                "sipx-clstr: {path} was refused — {} problem(s):",
                problems.len()
            );
            for problem in &problems {
                eprintln!("  {problem}");
            }
            return ExitCode::from(2);
        }
        Err(error) => {
            eprintln!("sipx-clstr: {error}");
            return ExitCode::from(2);
        }
    };

    let Ok(runtime) = tokio::runtime::Runtime::new() else {
        eprintln!("sipx-clstr: could not start the async runtime");
        return ExitCode::FAILURE;
    };

    // The driver announces the address once it is actually bound.
    match runtime.block_on(sipx_clstr_node::driver::run(config)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("sipx-clstr: {error}");
            ExitCode::FAILURE
        }
    }
}
