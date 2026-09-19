//! Built-in detectors, one module per supported stack or configuration source.
//!
//! Adding a new stack means adding a module here and registering it in
//! [`default_detectors`]; no existing detector has to change.

pub mod alembic;
pub mod docker;
pub mod environment;
pub mod node;
pub mod prisma;
pub mod python;
pub mod rust;

use crate::Detector;

/// All detectors known to this Pilot build, in deterministic evaluation order.
///
/// The order matters only for documentation purposes: every detector owns a
/// distinct part of the project model.
pub fn default_detectors() -> Vec<Box<dyn Detector>> {
    vec![
        Box::new(node::NodeDetector),
        Box::new(python::PythonDetector),
        Box::new(rust::RustDetector),
        Box::new(prisma::PrismaDetector),
        Box::new(alembic::AlembicDetector),
        Box::new(docker::DockerfileDetector),
        Box::new(docker::DockerComposeDetector),
        Box::new(environment::EnvironmentDetector),
    ]
}