//! Frame timing, for measuring the app (`cargo build --release --features perf`).
//!
//! Each second it adds a line to `perf.log` in the data folder: how many frames the
//! window drew, the average and longest frame (from the start of `render` to the end of
//! painting), and the time spent in each timed step, such as taking the core's events
//! or rebuilding the chat's rows. Without the feature every call here is empty.

#[cfg(feature = "perf")]
mod on {
    use std::cell::RefCell;
    use std::collections::BTreeMap;
    use std::io::Write;
    use std::time::{Duration, Instant};

    #[derive(Default)]
    struct Spent {
        count: u64,
        total: Duration,
        most: Duration,
    }

    impl Spent {
        fn add(&mut self, d: Duration) {
            self.count += 1;
            self.total += d;
            self.most = self.most.max(d);
        }
    }

    struct State {
        since: Instant,
        frame: Option<Instant>,
        frames: Spent,
        steps: BTreeMap<&'static str, Spent>,
    }

    thread_local! {
        static STATE: RefCell<State> = RefCell::new(State {
            since: Instant::now(),
            frame: None,
            frames: Spent::default(),
            steps: BTreeMap::new(),
        });
    }

    fn ms(d: Duration) -> f64 {
        d.as_secs_f64() * 1000.0
    }

    fn flush(s: &mut State) {
        if s.since.elapsed() < Duration::from_secs(1) {
            return;
        }
        let secs = s.since.elapsed().as_secs_f64();
        let mut line = format!(
            "frames {:.0}/s avg {:.2} ms max {:.2} ms",
            s.frames.count as f64 / secs,
            if s.frames.count > 0 {
                ms(s.frames.total) / s.frames.count as f64
            } else {
                0.0
            },
            ms(s.frames.most),
        );
        for (name, spent) in &s.steps {
            line.push_str(&format!(
                " | {name} {}x {:.2} ms max {:.2}",
                spent.count,
                ms(spent.total),
                ms(spent.most)
            ));
        }
        let path = std::path::Path::new(&crate::app::data_dir()).join("perf.log");
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
        {
            let _ = writeln!(f, "{line}");
        }
        s.since = Instant::now();
        s.frames = Spent::default();
        s.steps.clear();
    }

    static STARTED: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();

    thread_local! {
        static ONCE: RefCell<std::collections::HashSet<&'static str>> = RefCell::new(Default::default());
    }

    /// Notes when the app started, for `once`.
    pub fn start() {
        let _ = STARTED.set(Instant::now());
    }

    /// Writes how long after the start `name` first happened, the first time only.
    pub fn once(name: &'static str) {
        if !ONCE.with(|o| o.borrow_mut().insert(name)) {
            return;
        }
        let Some(at) = STARTED.get() else { return };
        let path = std::path::Path::new(&crate::app::data_dir()).join("perf.log");
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
        {
            let _ = writeln!(f, "{name} at {:.0} ms", ms(at.elapsed()));
        }
    }

    pub fn frame_start() {
        STATE.with(|s| s.borrow_mut().frame = Some(Instant::now()));
    }

    pub fn render_end() {
        STATE.with(|s| {
            let mut s = s.borrow_mut();
            if let Some(at) = s.frame {
                let d = at.elapsed();
                s.steps.entry("render").or_default().add(d);
            }
        });
    }

    pub fn frame_end() {
        STATE.with(|s| {
            let mut s = s.borrow_mut();
            if let Some(at) = s.frame.take() {
                s.frames.add(at.elapsed());
            }
            once("first frame");
            flush(&mut s);
        });
    }

    pub fn step<T>(name: &'static str, f: impl FnOnce() -> T) -> T {
        let at = Instant::now();
        let out = f();
        let d = at.elapsed();
        STATE.with(|s| {
            let mut s = s.borrow_mut();
            s.steps.entry(name).or_default().add(d);
            flush(&mut s);
        });
        out
    }
}

#[cfg(feature = "perf")]
pub use on::{frame_end, frame_start, once, start, step};

#[cfg(not(feature = "perf"))]
#[inline(always)]
pub fn start() {}

#[cfg(not(feature = "perf"))]
#[inline(always)]
pub fn once(_name: &'static str) {}

#[cfg(not(feature = "perf"))]
#[inline(always)]
pub fn frame_start() {}

#[cfg(not(feature = "perf"))]
#[inline(always)]
pub fn frame_end() {}

#[cfg(not(feature = "perf"))]
#[inline(always)]
pub fn step<T>(_name: &'static str, f: impl FnOnce() -> T) -> T {
    f()
}

/// Counts that `name` happened, as a step that takes no time.
#[inline(always)]
pub fn count(name: &'static str) {
    step(name, || ());
}

/// Notes that `render` has built the window's elements, as the step "render": the
/// rest of the frame is GPUI's layout and painting.
#[cfg(feature = "perf")]
pub fn render_end() {
    on::render_end();
}

#[cfg(not(feature = "perf"))]
#[inline(always)]
pub fn render_end() {}
