//! A termination signal, turned into something the draw loop can read.
//!
//! `ratatui::init` restores the terminal on panic and `main` restores it on
//! every `Result` path, but a signal's default disposition ends the process
//! without running either: `kill`, a closed SSH session, or a terminal
//! window closing all left the tty in raw mode on the alternate screen. The
//! handler here does nothing but record the signal's number; the loop
//! notices it between frames and returns, so the exit runs through the same
//! restore as `q` does.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

/// The number of the signal that arrived, or zero while none has. A number
/// rather than a bool so `main` can exit with the status a shell expects of
/// a process a signal ended. An `Arc` because that is the only shape the
/// handler registration takes; the loop reads it through the same handle.
#[derive(Clone, Debug, Default)]
pub struct Interrupt(Arc<AtomicUsize>);

impl Interrupt {
    pub fn new() -> Self {
        Self::default()
    }

    /// Hook SIGTERM, SIGHUP and SIGINT. SIGINT because a few terminals send
    /// the signal instead of the Ctrl-C key event raw mode normally turns it
    /// into. Unix only: Windows has none of these, and crossterm already
    /// reports Ctrl-C and Ctrl-Break there as key events.
    #[cfg(unix)]
    pub fn register(&self) -> std::io::Result<()> {
        use signal_hook::consts::{SIGHUP, SIGINT, SIGTERM};
        for signal in [SIGTERM, SIGHUP, SIGINT] {
            signal_hook::flag::register_usize(signal, Arc::clone(&self.0), signal as usize)?;
        }
        Ok(())
    }

    /// The signal that arrived, if one has.
    pub fn signal(&self) -> Option<i32> {
        // A signal number fits in an i32 by definition: that is the type the
        // handler was registered with.
        match self.0.load(Ordering::SeqCst) {
            0 => None,
            signal => Some(signal as i32),
        }
    }

    /// What the handler does, without a signal to do it.
    #[cfg(test)]
    pub fn raise(&self, signal: i32) {
        self.0.store(signal as usize, Ordering::SeqCst);
    }
}

/// The status a shell reports for a process a signal ended: 128 plus the
/// signal's number, so `kill -TERM` reads as 143 and an interrupt as 130.
/// The process exits on its own rather than re-raising the signal with the
/// default disposition, because by then the terminal has been restored and
/// the notices printed, and a re-raise would report the death faithfully at
/// the cost of running none of that.
pub fn exit_status(signal: i32) -> i32 {
    128 + signal
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nothing_has_arrived_until_something_does() {
        let interrupt = Interrupt::new();
        assert_eq!(interrupt.signal(), None);

        interrupt.raise(15);
        assert_eq!(interrupt.signal(), Some(15));
    }

    /// The loop reads the flag on every pass; a signal must stay noticed
    /// rather than be consumed by the first read.
    #[test]
    fn a_signal_stays_raised_across_reads() {
        let interrupt = Interrupt::new();
        interrupt.raise(1);
        assert_eq!(interrupt.signal(), Some(1));
        assert_eq!(interrupt.signal(), Some(1));
    }

    /// The handler writes through its own clone of the handle; the loop must
    /// see that write through the original.
    #[test]
    fn every_clone_shares_one_flag() {
        let interrupt = Interrupt::new();
        let handler = interrupt.clone();
        handler.raise(2);
        assert_eq!(interrupt.signal(), Some(2));
    }

    #[test]
    fn the_exit_status_is_the_shell_convention() {
        assert_eq!(exit_status(1), 129, "SIGHUP");
        assert_eq!(exit_status(2), 130, "SIGINT");
        assert_eq!(exit_status(15), 143, "SIGTERM");
    }
}
