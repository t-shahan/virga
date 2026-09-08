//! A termination signal, turned into something the draw loop can read.
//!
//! `ratatui::init` restores the terminal on panic and `main` restores it on
//! every `Result` path, but a signal's default disposition ends the process
//! without running either: a `kill` left the tty in raw mode on the
//! alternate screen. The handler here does nothing but record the signal's
//! number; the loop notices it between frames and returns, so the exit runs
//! through the same restore as `q` does, and `main` then raises the signal
//! again so the death is reported the way it would have been.
//!
//! SIGHUP is left alone on purpose; `Interrupt::register` says why.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

/// The number of the signal that arrived, or zero while none has. A number
/// rather than a bool so `main` can re-raise the one that arrived. An `Arc`
/// because that is the only shape the handler registration takes; the loop
/// reads it through the same handle.
#[derive(Clone, Debug, Default)]
pub struct Interrupt(Arc<AtomicUsize>);

impl Interrupt {
    pub fn new() -> Self {
        Self::default()
    }

    /// Hook SIGTERM and SIGINT. SIGINT because a few terminals send the
    /// signal instead of the Ctrl-C key event raw mode normally turns it
    /// into. Unix only: Windows has none of these, and crossterm already
    /// reports Ctrl-C and Ctrl-Break there as key events.
    ///
    /// SIGHUP keeps its default disposition. It arrives once the terminal is
    /// already gone (a window closed, an SSH session dropped), so there is
    /// nothing left to restore and nobody to see it, and hooking it is a
    /// trap: crossterm 0.29's event source, once epoll reports the tty
    /// readable, loops on `read` and breaks out only for `WouldBlock`, so
    /// the `0` a hung-up tty returns forever keeps `event::poll` from ever
    /// coming back, and the loop never reaches the check that would end it.
    /// Hooked, a closed window left an orphaned virga at 100% CPU; unhooked,
    /// the hangup ends it as it always did. The one case a hook would serve,
    /// a hand-typed `kill -HUP`, is not worth that.
    #[cfg(unix)]
    pub fn register(&self) -> std::io::Result<()> {
        use signal_hook::consts::{SIGINT, SIGTERM};
        for signal in [SIGTERM, SIGINT] {
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

/// End the process the way the signal would have, now that the terminal is
/// restored and the notices printed. The handler goes back to its default
/// and the signal is raised again, so whatever waits on virga sees a death
/// by that signal: bash abandons a script whose foreground child died of
/// SIGINT, and `timeout`, make and systemd all read `WIFSIGNALED`. The cost
/// is a noisier prompt, `Terminated` after a `kill`, which is the truth.
///
/// The alternative, `exit(128 + signal)`, is the number a shell prints for
/// the same death and most scripts treat the two alike. It was passed over
/// because it turns a SIGINT into a plain exit, so a script that a Ctrl-C
/// should stop carries on past virga instead. It stays as the fallback for a
/// re-raise that returns, which means the signal did not end the process.
pub fn end(signal: i32) -> ! {
    #[cfg(unix)]
    if let Err(error) = signal_hook::low_level::emulate_default_handler(signal) {
        eprintln!("virga: could not re-raise signal {signal}: {error}");
    }
    std::process::exit(exit_status(signal))
}

/// The status a shell reports for a process a signal ended: 128 plus the
/// signal's number, so `kill -TERM` reads as 143 and an interrupt as 130.
fn exit_status(signal: i32) -> i32 {
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
        interrupt.raise(15);
        assert_eq!(interrupt.signal(), Some(15));
        assert_eq!(interrupt.signal(), Some(15));
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
        assert_eq!(exit_status(2), 130, "SIGINT");
        assert_eq!(exit_status(15), 143, "SIGTERM");
    }
}
