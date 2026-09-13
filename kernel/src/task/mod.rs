pub mod context;
pub mod scheduler;
pub mod switch;
pub mod process;

pub use context::{Context, InterruptContext, SavedThreadState};
pub use scheduler::{Thread, Scheduler, SCHEDULER};
pub use switch::*;
pub use process::Process;

pub fn init() {
    log::info!("[TASK] Initializing scheduler and task subsystem");
    // Any initialization logic if needed
}
