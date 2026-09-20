//! A tripwire for reentrant `db()` locking, debug builds only.
//!
//! `Engine::db()` hands out a guard over the one SQLite connection. `Mutex` is
//! not reentrant, so taking that lock twice on one thread does not fail — it
//! blocks forever while still holding the first acquisition, and every other
//! database user in the process, including the command that would have drawn
//! the window, queues behind it. The engine's own note calls the result *"a
//! total freeze, not a slow path"*, and the shape that caused it there is a
//! guard bound by an `if let Ok(Some(..)) = self.db().…` scrutinee, whose
//! lifetime runs to the end of the block, with a second `self.db()` inside the
//! body.
//!
//! A deadlock leaves no evidence: the process simply stops, and nothing says
//! which call site closed the loop. So this module turns that case into a panic
//! naming both the outer and the inner call site, which a test or a dev run
//! hits immediately.
//!
//! **Debug only, and that is a real limit.** In a release build the lock is
//! taken exactly as before, so a reentrant `db()` still freezes the shipped
//! app. This finds the bug; it does not fix it. Fixing it means removing the
//! reentrant call sites, or moving off one shared connection.
//!
//! # Why the call site is a parameter
//!
//! `Location::caller()` read *inside* this module resolves to the call to this
//! module, not to the function that asked for the database. Since `lock` is
//! called from exactly one place — the body of `db()` — that is a single line
//! shared by every database user in the process, so a message built that way
//! would name that one line for both the outer and the inner lock and say
//! nothing at all about who closed the loop.
//!
//! An earlier version of this file did exactly that, read the location itself,
//! and carried a test that passed because it asserted this module's own
//! filename appeared in the message. The test was pinned to the bug.
//!
//! So the site is passed in. `db()` is `#[track_caller]`, which is what makes
//! `Location::caller()` *there* mean the caller of `db()` rather than the line
//! inside it, and that location is the one worth printing.

#[cfg(debug_assertions)]
use std::cell::Cell;
use std::ops::{Deref, DerefMut};
use std::panic::Location;
use std::sync::{Mutex, MutexGuard};

#[cfg(debug_assertions)]
thread_local! {
    /// Where this thread's live database guard was taken, if it holds one.
    /// `None` means this thread is outside the critical section.
    static HELD: Cell<Option<&'static Location<'static>>> = const { Cell::new(None) };
}

/// A live database guard.
///
/// Derefs to the guarded value, so it stands in for the plain `MutexGuard` at
/// every call site.
pub struct Guard<'a, T> {
    /// Proof that this thread entered the critical section, released when the
    /// guard drops. Debug-only, and zero-sized in release.
    _held: Held,
    inner: MutexGuard<'a, T>,
}

impl<T> Deref for Guard<'_, T> {
    type Target = T;

    fn deref(&self) -> &T {
        &self.inner
    }
}

impl<T> DerefMut for Guard<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        &mut self.inner
    }
}

/// Take the lock for `site`, refusing a reentrant one.
///
/// The check runs **before** `lock`, and that order is the point: a second
/// `lock` on this thread is the hang itself, so waiting to find out by taking
/// it would be the bug rather than the detector.
/// `a_second_lock_refuses_instead_of_hanging` fails by hanging if this is ever
/// swapped.
///
/// A poisoned mutex is recovered rather than re-panicked. `expect("db mutex
/// poisoned")` here meant that any panic while a database guard was live
/// poisoned the connection for the rest of the process, and every later `db()`
/// in the app — drawing a chat, saving a setting, listing sessions — then
/// panicked on this line instead. One bad turn became a Loom that could not
/// touch its own database again. The connection is the state to prefer: the
/// transaction a `rusqlite` guard owns rolls back on unwind, so what is left is
/// the last committed data, which is the best answer available and strictly
/// better than having no database at all.
///
/// Not `#[track_caller]`: the caller's location is already an argument, and
/// reading it here as well would only produce the second, useless one described
/// in the module comment.
pub fn lock<'a, T>(
    mutex: &'a Mutex<T>,
    site: &'static Location<'static>,
) -> Guard<'a, T> {
    Guard {
        // Field order matters, and not only for the borrow checker: `enter`
        // must run before `lock`, or the tripwire would be checking after the
        // hang rather than before it.
        _held: Held::enter(site),
        inner: mutex.lock().unwrap_or_else(|poisoned| poisoned.into_inner()),
    }
}

/// The record that this thread is inside a database critical section.
///
/// A unit type in both builds: in release it carries no check and no drop, so
/// the marker costs nothing and the guard stays exactly a `MutexGuard`.
struct Held;

#[cfg(debug_assertions)]
impl Held {
    /// Mark the section at `site`, or panic naming both call sites if one is
    /// already live.
    fn enter(site: &'static Location<'static>) -> Self {
        HELD.with(|held| {
            if let Some(there) = held.get() {
                panic!(
                    "loom: reentrant db() lock, which would freeze the app\n  \
                     already held at: {there}\n  \
                     locked again at: {site}\n  \
                     A second db() on one thread blocks forever holding the first, and \
                     every other database user in the process queues behind it. Read what \
                     you need into a local and let the first guard drop first — see the \
                     note on Engine::db."
                );
            }
            held.set(Some(site));
        });
        Held
    }
}

#[cfg(debug_assertions)]
impl Drop for Held {
    fn drop(&mut self) {
        HELD.with(|held| held.set(None));
    }
}

#[cfg(not(debug_assertions))]
impl Held {
    /// Nothing to mark and nothing to check in a release build. The site is
    /// still required, so both builds call this the same way and there is no
    /// `cfg` in `db()` to drift out of step with the check.
    #[inline]
    fn enter(_site: &'static Location<'static>) -> Self {
        Held
    }
}

#[cfg(all(test, debug_assertions))]
mod tests {
    use super::*;

    /// Two guards on one thread. The second must refuse, because a `Mutex` will
    /// not tell us on its own: it simply blocks, still holding the first.
    ///
    /// This test hangs rather than fails if the check ever moves after the
    /// `lock` call, which is the one ordering mistake worth pinning.
    #[test]
    #[should_panic(expected = "reentrant db() lock")]
    fn a_second_lock_refuses_instead_of_hanging() {
        let mutex = Mutex::new(7u32);
        let first = lock(&mutex, Location::caller());
        assert_eq!(*first, 7);
        let _second = lock(&mutex, Location::caller());
    }

    /// The mark has to be released when the guard drops, or every later lock in
    /// the process would refuse — a false alarm in the one place that cannot
    /// afford one.
    #[test]
    fn the_lock_can_be_taken_again_once_the_guard_drops() {
        let mutex = Mutex::new(7u32);
        {
            let first = lock(&mutex, Location::caller());
            assert_eq!(*first, 7);
        }
        let again = lock(&mutex, Location::caller());
        assert_eq!(*again, 7);
    }

    /// A write through the guard still reaches the value, since every call site
    /// now goes through `DerefMut` rather than owning a `MutexGuard`.
    #[test]
    fn the_guard_still_writes_through() {
        let mutex = Mutex::new(1u32);
        {
            let mut guard = lock(&mutex, Location::caller());
            *guard = 2;
        }
        assert_eq!(*lock(&mutex, Location::caller()), 2);
    }

    /// A guard dropped by unwinding must still clear the mark, and the lock
    /// must still be usable afterwards — otherwise one panicking `db()` body
    /// would make every later `db()` in the process refuse, or panic, in the
    /// one place that cannot afford either.
    ///
    /// Both halves are one test because they are one question: what is left
    /// after an unwind with the guard live. This test failed against the
    /// version that used `.expect("db mutex poisoned")` — the second lock
    /// panicked with `PoisonError`, which is what the recovery is for.
    #[test]
    fn an_unwind_clears_the_mark_and_leaves_the_lock_usable() {
        let mutex = Mutex::new(0u32);
        let panicked = std::panic::catch_unwind(|| {
            let _guard = lock(&mutex, Location::caller());
            panic!("body panics with the guard live");
        });
        assert!(panicked.is_err());
        // The same mutex, not a fresh one: the point is that the poison does
        // not wedge it.
        assert_eq!(*lock(&mutex, Location::caller()), 0);
    }

    /// The recovery itself, asserted against a mutex poisoned by hand, so it
    /// cannot regress by someone reinstating `.expect("db mutex poisoned")`
    /// with a comment that sounds like a good reason.
    #[test]
    fn a_poisoned_mutex_is_taken_back_rather_than_re_panicking() {
        let mutex = Mutex::new(0u32);
        let _ = std::panic::catch_unwind(|| {
            let _guard = mutex.lock().expect("not poisoned yet");
            panic!("poison it");
        });
        assert!(
            mutex.lock().is_err(),
            "a panic with the guard live must poison the mutex, or this test \
             is not testing anything"
        );
        assert_eq!(*lock(&mutex, Location::caller()), 0);
    }

    /// The message must name the sites it was **given**, and this is the test
    /// that pins the case above.
    ///
    /// The earlier version asserted only that `db_reentry.rs` appeared in the
    /// message, which was true of the broken behaviour too — the module was
    /// naming its own line. Two sites on two different lines can only both
    /// appear if the passed-in locations are what gets printed, so this fails
    /// if the location is ever read inside the module again.
    #[test]
    fn the_refusal_names_the_two_sites_it_was_given() {
        let mutex = Mutex::new(0u32);
        let outer = Location::caller();
        let inner = Location::caller();
        assert_ne!(
            outer.line(),
            inner.line(),
            "the test needs two distinct sites to be able to tell them apart"
        );
        let payload = std::panic::catch_unwind(|| {
            let _first = lock(&mutex, outer);
            let _second = lock(&mutex, inner);
        })
        .expect_err("a reentrant lock must panic");
        let message = payload
            .downcast_ref::<String>()
            .cloned()
            .unwrap_or_default();
        assert!(message.contains("already held at:"), "{message}");
        assert!(message.contains("locked again at:"), "{message}");
        assert!(message.contains(&outer.to_string()), "{message}");
        assert!(message.contains(&inner.to_string()), "{message}");
    }
}
