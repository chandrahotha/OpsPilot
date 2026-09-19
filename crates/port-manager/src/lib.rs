//! Pilot Port Manager - Port detection and conflict handling
//!
//! Detects configured application ports, ports currently in use, and reports
//! whether a port is free. Inspection is cross-platform and read-only: Pilot
//! never kills a process it does not own.
//!
//! Process ownership (PID and process name) requires platform tooling and is
//! scheduled for the ports phase; [`PortStatus::pid`] and [`PortStatus::process`]
//! stay `None` until then. Reporting an unverified owner would violate the
//! "no claims without evidence" rule of "Pilot Prerequisite.md" section 17.

use std::net::{Ipv4Addr, SocketAddrV4, TcpListener, TcpStream};
use std::time::Duration;

/// Timeout used when probing a port. Kept short so inspection never blocks the UI
pub const PROBE_TIMEOUT: Duration = Duration::from_millis(250);

/// How far above a preferred port Pilot searches for a free port
pub const SEARCH_RANGE: u16 = 20;

/// Port status information
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PortStatus {
    /// The port number
    pub port: u16,
    /// Whether the port is available
    pub available: bool,
    /// The process ID occupying the port (if occupied)
    pub pid: Option<u32>,
    /// The process name occupying the port (if occupied)
    pub process: Option<String>,
}

/// Port management operations
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PortOperation {
    /// Find a free port near the specified port
    FindFreePort(u16),
    /// Inspect a port to see if it's in use
    InspectPort(u16),
    /// Change a port configuration in project files
    ChangePort(u16, u16),
}

/// Outcome of a port operation
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PortOutcome {
    /// The inspected port state
    Status(PortStatus),
    /// A free port was found
    FreePort(u16),
    /// The operation is not implemented yet
    NotImplemented(&'static str),
}

/// Whether something is accepting TCP connections on the loopback interface
pub fn is_listening(port: u16) -> bool {
    let address = SocketAddrV4::new(Ipv4Addr::LOCALHOST, port);

    TcpStream::connect_timeout(&address.into(), PROBE_TIMEOUT).is_ok()
}

/// Whether a port can be bound, i.e. it is free for a new service
pub fn is_bindable(port: u16) -> bool {
    TcpListener::bind((Ipv4Addr::LOCALHOST, port)).is_ok()
}

/// Inspect the current state of a port
pub fn inspect_port(port: u16) -> PortStatus {
    let occupied = is_listening(port) || !is_bindable(port);

    PortStatus {
        port,
        available: !occupied,
        // Ownership lookup is a platform operation and is not implemented yet.
        pid: None,
        process: None,
    }
}

/// Find the first free port at or above `preferred`
///
/// Returns `None` when every port in [`SEARCH_RANGE`] is taken.
pub fn find_free_port(preferred: u16) -> Option<u16> {
    let last = preferred.saturating_add(SEARCH_RANGE);

    (preferred..=last).find(|port| inspect_port(*port).available)
}

/// Execute a port operation
pub fn execute(operation: PortOperation) -> PortOutcome {
    match operation {
        PortOperation::InspectPort(port) => PortOutcome::Status(inspect_port(port)),
        PortOperation::FindFreePort(port) => match find_free_port(port) {
            Some(free) => PortOutcome::FreePort(free),
            None => PortOutcome::NotImplemented("no free port found in search range"),
        },
        // Rewriting project configuration is only safe once every relevant
        // reference can be identified, so it stays unimplemented on purpose.
        PortOperation::ChangePort(..) => {
            PortOutcome::NotImplemented("changing a configured port is not implemented yet")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, MutexGuard, OnceLock};

    /// Port tests bind and release real ports. Running them in parallel would let
    /// one test take a port another test just released, so they are serialized.
    fn port_lock() -> MutexGuard<'static, ()> {
        static PORT_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

        PORT_LOCK
            .get_or_init(|| Mutex::new(()))
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Wait until the operating system has actually released a port
    fn wait_until_free(port: u16) -> bool {
        for _ in 0..50 {
            if inspect_port(port).available {
                return true;
            }

            std::thread::sleep(Duration::from_millis(10));
        }

        false
    }

    #[test]
    fn a_bound_port_is_reported_as_occupied() {
        let _lock = port_lock();

        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).expect("must bind");
        let port = listener.local_addr().expect("must have an address").port();

        assert!(is_listening(port));

        let status = inspect_port(port);

        assert_eq!(status.port, port);
        assert!(!status.available);
        assert!(status.pid.is_none(), "ownership must not be claimed");
        assert!(status.process.is_none());
    }

    #[test]
    fn a_released_port_becomes_available_again() {
        let _lock = port_lock();

        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).expect("must bind");
        let port = listener.local_addr().expect("must have an address").port();
        drop(listener);

        assert!(
            wait_until_free(port),
            "port {port} should be free again after the listener is dropped"
        );

        let status = inspect_port(port);

        assert_eq!(status.port, port);
        assert!(status.available);
    }

    #[test]
    fn free_port_search_skips_occupied_ports() {
        let _lock = port_lock();

        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).expect("must bind");
        let occupied = listener.local_addr().expect("must have an address").port();

        let free = find_free_port(occupied).expect("a free port must be found in range");

        assert!(free > occupied);
        assert!(inspect_port(free).available);
    }

    #[test]
    fn changing_a_port_is_reported_as_not_implemented() {
        assert_eq!(
            execute(PortOperation::ChangePort(3000, 3100)),
            PortOutcome::NotImplemented("changing a configured port is not implemented yet")
        );
    }

    #[test]
    fn inspecting_through_the_operation_layer_returns_a_status() {
        let _lock = port_lock();

        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).expect("must bind");
        let port = listener.local_addr().expect("must have an address").port();

        match execute(PortOperation::InspectPort(port)) {
            PortOutcome::Status(status) => assert!(!status.available),
            other => panic!("expected a status, got {other:?}"),
        }
    }
}