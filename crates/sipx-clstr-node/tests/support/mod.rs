//! What a driver test needs in order to have a listener that cannot collide (`CF-13`).
//!
//! Every integration test in this crate needs a node on a real socket, and each one used to pick a
//! port by hand: `15081`, `15091`, `15071`. That works exactly as long as the suite is the only
//! thing running. Two worktrees testing at once bind the same numbers and one of them loses, with
//! `Address already in use (os error 98)` landing in whichever diff happened to be under test —
//! the worst possible place for it, because the natural reading is "my change broke this". The
//! workaround does not scale, either: `fork_branches.rs` chose `15091`/`15092` *to dodge* the known
//! ports, and `auth_observable.rs` then picked the same pair.
//!
//! So no test picks a port. A node binds `127.0.0.1:0`, the kernel assigns one out of the ephemeral
//! range, and the test asks the node what it got. Two suites can then run at once and never meet,
//! however many worktrees there are.
//!
//! **Where the answer comes from.** The node already publishes it: `listening on <addr>` goes to
//! stdout after the bind and after every startup refusal, precisely so a caller need not guess —
//! `scripts/e2e-call.sh` and `website/docs/guides/run-a-node.md` both wait on that line. A test that
//! runs the **binary** reads it from stdout; a test that runs the driver **in process** cannot read
//! its own stdout, so it gets the same value from the same place through
//! [`driver::run_reporting`]. Neither invents a second contract.
//!
//! **Why the advertised address is still a literal.** `Advertised` refuses port zero, and the
//! advertised address is decided before the bind, so it cannot be the assigned port. It does not
//! need to be: `DP-5` makes bind and advertise independent by design, and nothing in this suite
//! routes a message *to* the advertised address — the devices answer the source address they
//! received from, and no test dials a `Record-Route`. Only the bind can collide, and the bind is
//! now the kernel's to choose.

// Each test binary compiles this whole module and uses part of it. Denying dead code here would
// mean maintaining one `cfg` per consumer, which is a worse trade than the warning.
#![allow(dead_code)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::io::{BufRead, BufReader, Write};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use sipx_clstr_node::driver::{self, NodeConfig, NodeError};

/// The bind address every node in this suite is given: loopback, and a port the kernel picks.
pub(crate) const EPHEMERAL: &str = "127.0.0.1:0";

/// How long a helper waits for a node to say something before calling it dead.
///
/// Generous on purpose. This is not a budget the assertions depend on — every wait here returns as
/// soon as the thing it waits for has happened — it is only the point at which "still nothing"
/// becomes a better report than hanging. Under a heavy parallel fan-out a debug-built node can take
/// seconds to reach its first log line, and a tight bound here would recreate, as a timeout, exactly
/// the load-sensitive flake this module exists to remove.
pub(crate) const PATIENCE: Duration = Duration::from_secs(30);

/// [`EPHEMERAL`], parsed.
#[must_use]
pub(crate) fn ephemeral() -> SocketAddr {
    EPHEMERAL.parse().expect("a loopback address")
}

/// A scratch directory for one test, under this process and this tag.
///
/// Per test rather than per suite because `/tmp` here is a tmpfs shared across worktrees, and it
/// filled during the session that produced `CF-13` — two runs writing `cluster.yaml` to one path is
/// the same class of shared-resource collision as two runs binding one port.
#[must_use]
pub(crate) fn scratch_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("sipx-clstr-test-{}-{tag}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("a scratch directory");
    dir
}

/// Write `text` as a cluster document under [`scratch_dir`], and return its path.
#[must_use]
pub(crate) fn write_document(text: &str, tag: &str) -> String {
    let path = scratch_dir(tag).join("cluster.yaml");
    let mut file = std::fs::File::create(&path).expect("create the document");
    file.write_all(text.as_bytes()).expect("write the document");
    drop(file);
    path.to_str().expect("a utf-8 path").to_owned()
}

// ------------------------------------------------------------------------------- in process ---

/// A node running in this process, and the address it bound.
pub(crate) struct InProcessNode {
    listening: SocketAddr,
    running: tokio::task::JoinHandle<Result<(), NodeError>>,
}

impl InProcessNode {
    /// Where to send. Whatever the kernel assigned, never a number this suite chose.
    #[must_use]
    pub(crate) fn addr(&self) -> SocketAddr {
        self.listening
    }

    /// The same address as text, for the many messages here that are built as strings.
    #[must_use]
    pub(crate) fn target(&self) -> String {
        self.listening.to_string()
    }

    /// Stop it. Named rather than left to `Drop` because every one of these tests aborted its node
    /// explicitly before, and a silent change of shutdown order is not worth the brevity.
    pub(crate) fn stop(self) {
        self.running.abort();
    }
}

/// Start `config` on the driver and wait until it reports the address it bound.
///
/// Waiting on the report rather than on a clock is the point: it is the node's own readiness signal,
/// so this returns as soon as the node is serving and never sooner. A node that refuses to start
/// returns its error here instead of leaving the test to retry into a socket that will never answer.
///
/// # Panics
///
/// If the node fails to start, or stops before reporting.
pub(crate) async fn start_in_process(config: NodeConfig) -> InProcessNode {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let running = tokio::spawn(async move {
        driver::run_reporting(config, |addr| {
            let _ = tx.send(addr);
        })
        .await
    });

    let Ok(reported) = tokio::time::timeout(PATIENCE, rx).await else {
        panic!("the node did not report a bound address within {PATIENCE:?}")
    };
    match reported {
        Ok(listening) => InProcessNode { listening, running },
        // The sender was dropped, which can only mean `run_reporting` returned before announcing.
        // Whatever it returned is the reason, and it is worth more than "the channel closed".
        Err(_) => match running.await {
            Ok(Err(error)) => panic!("the node refused to start: {error}"),
            Ok(Ok(())) => panic!("the node stopped before it bound anything"),
            Err(error) => panic!("the node's task failed: {error}"),
        },
    }
}

// ------------------------------------------------------------------------------ the binary ---

/// The node binary, running as a child process, with its two streams read as they arrive.
///
/// Reading rather than sleeping is what makes the tests that use this insensitive to load. The
/// previous shape — sleep a fixed 1500 ms, kill, then assert on whatever had been written — asserted
/// on a *race* between a wall clock and a 500 ms sampling tick, and lost it about one run in five
/// under a parallel fan-out. Nothing here waits for a duration; everything waits for an output.
/// **Both** streams are drained for the node's whole life, on threads of their own, and neither is
/// closed until the node is killed. That is not tidiness: a pipe whose read end is dropped makes the
/// child's next write fail with `EPIPE`, and the node prints `advertising <addr>` immediately after
/// `listening on <addr>`. Reading one line and dropping the reader therefore killed the node about
/// as often as the scheduler split those two `println!`s — which is to say, under load. A full pipe
/// does the mirror-image damage: the child blocks on its next log line, and a node blocked in
/// `tracing` has stopped serving for reasons entirely of the harness's making.
///
/// Neither pipe is held here, only its accumulated text: the draining threads own the handles, and
/// they are what keeps the pipes open.
pub(crate) struct BinaryNode {
    child: Child,
    listening: SocketAddr,
    stderr: Arc<Mutex<String>>,
}

impl BinaryNode {
    /// Run `sipx-clstr run --config <path>` as node 1 in zone a, and wait for it to bind.
    ///
    /// `RUST_LOG` is removed deliberately, as it was in every test that used to do this by hand: an
    /// operator who has to know to turn logging up has not been told, so what these tests read is
    /// what the default level emits.
    ///
    /// # Panics
    ///
    /// If the binary does not start, or exits before printing `listening on` — in which case the
    /// panic carries its stderr, because that is where the reason is.
    #[must_use]
    pub(crate) fn start(config_path: &str) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_sipx-clstr"))
            .args([
                "run",
                "--config",
                config_path,
                "--node",
                "1",
                "--zone",
                "a",
                "--roles",
                "edge,registrar",
            ])
            .env_remove("RUST_LOG")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("the binary runs");

        let stdout = drain(child.stdout.take().expect("a piped stdout"));
        let stderr = drain(child.stderr.take().expect("a piped stderr"));

        let listening = await_listening_line(&mut child, &stdout, &stderr);
        Self {
            child,
            listening,
            stderr,
        }
    }

    /// The address the node bound, as it reported it on stdout.
    #[must_use]
    pub(crate) fn addr(&self) -> SocketAddr {
        self.listening
    }

    /// Everything the node has written to stderr so far.
    #[must_use]
    pub(crate) fn stderr(&self) -> String {
        self.stderr
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    /// Wait until stderr satisfies `ready`, and return it — or return whatever there is after
    /// [`PATIENCE`], so the assertion that follows reports the real output rather than a timeout.
    ///
    /// This is the load-insensitive replacement for "sleep past the sampling interval and hope".
    #[must_use]
    pub(crate) fn stderr_until(&self, ready: impl Fn(&str) -> bool) -> String {
        let deadline = Instant::now() + PATIENCE;
        loop {
            let seen = self.stderr();
            if ready(&seen) || Instant::now() >= deadline {
                return seen;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    /// Stop the node and return everything it ever wrote to stderr.
    pub(crate) fn stop(mut self) -> String {
        let _ = self.child.kill();
        let _ = self.child.wait();
        self.stderr()
    }
}

/// Drain `stream` to end-of-file on a thread, accumulating what it carried.
///
/// The handle is kept alive by the thread for as long as the child writes, which is the property
/// that matters: see [`BinaryNode`] on what closing one of these early does to the node.
fn drain(stream: impl std::io::Read + Send + 'static) -> Arc<Mutex<String>> {
    let seen = Arc::new(Mutex::new(String::new()));
    let sink = Arc::clone(&seen);
    std::thread::spawn(move || {
        let mut reader = BufReader::new(stream);
        let mut line = String::new();
        while reader.read_line(&mut line).unwrap_or(0) > 0 {
            sink.lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .push_str(&line);
            line.clear();
        }
    });
    seen
}

/// Wait for the child to announce its bound address on stdout.
///
/// The `listening on <addr>` line is the documented readiness signal, and it is printed after every
/// refusal that can stop a node starting — so seeing it means the node is serving, and seeing the
/// child exit without it means it is not and never will be.
///
/// # Panics
///
/// If the node exits first, or says nothing within [`PATIENCE`]. Either panic carries the node's own
/// stderr, because that is where the reason is.
fn await_listening_line(
    child: &mut Child,
    stdout: &Arc<Mutex<String>>,
    stderr: &Arc<Mutex<String>>,
) -> SocketAddr {
    let deadline = Instant::now() + PATIENCE;
    loop {
        let seen = stdout
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone();
        if let Some(addr) = seen
            .lines()
            .find_map(|line| line.trim().strip_prefix("listening on "))
        {
            return addr.parse().expect("the node announced a real address");
        }

        // Checked after reading, not before: a node can bind, announce and exit between two polls,
        // and the announcement is still the truth about what it bound.
        let exited = matches!(child.try_wait(), Ok(Some(_)));
        if exited || Instant::now() >= deadline {
            // Let the stderr thread catch up — the reason is there, and reporting "it exited"
            // without it would waste the reader's time.
            std::thread::sleep(Duration::from_millis(100));
            let why = stderr
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .clone();
            assert!(
                exited,
                "the node bound nothing within {PATIENCE:?}. stderr was:\n{why}"
            );
            panic!("the node exited before it bound anything. stderr was:\n{why}");
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}
