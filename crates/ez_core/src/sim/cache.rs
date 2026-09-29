//! Bakes without freezing the editor: on desktop each runs on a thread; in
//! the browser (no threads) they run a slice at a time from
//! [`BakeCache::poll`].

use super::{Bake, BakeJob};
use std::collections::HashMap;
use std::sync::Arc;

/// Simulation steps between checks of the time budget (or cancelling).
const CHUNK: usize = 64;

/// Bakes by slot (one per simulated layer, numbered by the caller) and key
/// (a hash of everything the bake depends on, see
/// [`KeyHasher`](super::KeyHasher)). While a slot's new bake runs, it keeps
/// handing out the previous one, so the preview doesn't blank out while a
/// setting is dragged.
pub struct BakeCache {
    threads: bool,
    slots: HashMap<u64, Slot>,
}

#[derive(Default)]
struct Slot {
    ready: Option<(u64, Arc<Bake>)>,
    work: Option<Work>,
    used: bool,
}

struct Work {
    key: u64,
    run: Run,
}

enum Run {
    Inline(Box<BakeJob>),
    #[cfg(not(target_arch = "wasm32"))]
    Thread {
        rx: std::sync::mpsc::Receiver<Bake>,
        progress: Arc<std::sync::atomic::AtomicU32>,
        _cancel: Cancel,
    },
}

/// Stops a bake's thread when its result is no longer wanted.
#[cfg(not(target_arch = "wasm32"))]
struct Cancel(Arc<std::sync::atomic::AtomicBool>);

#[cfg(not(target_arch = "wasm32"))]
impl Drop for Cancel {
    fn drop(&mut self) {
        self.0.store(true, std::sync::atomic::Ordering::Relaxed);
    }
}

impl Default for BakeCache {
    fn default() -> Self {
        Self::new()
    }
}

impl BakeCache {
    /// Bakes on threads where there are threads.
    pub fn new() -> BakeCache {
        BakeCache {
            threads: cfg!(not(target_arch = "wasm32")),
            slots: HashMap::new(),
        }
    }

    /// Bakes only inside [`BakeCache::poll`] and [`BakeCache::finish_all`].
    pub fn inline() -> BakeCache {
        BakeCache {
            threads: false,
            slots: HashMap::new(),
        }
    }

    /// The bake for `key` in `slot`, starting it with `job` if it isn't
    /// there or on its way. Until it is ready, the slot's previous bake (if
    /// any); [`BakeCache::is_ready`] tells which.
    pub fn get(&mut self, slot: u64, key: u64, job: impl FnOnce() -> BakeJob) -> Option<Arc<Bake>> {
        let threads = self.threads;
        let s = self.slots.entry(slot).or_default();
        s.used = true;
        match &s.ready {
            Some((k, bake)) if *k == key => return Some(bake.clone()),
            _ => {}
        }
        if s.work.as_ref().is_none_or(|w| w.key != key) {
            s.work = Some(Work {
                key,
                run: start(job(), threads),
            });
        }
        s.ready.as_ref().map(|(_, b)| b.clone())
    }

    /// Whether `slot` holds the bake for `key`.
    pub fn is_ready(&self, slot: u64, key: u64) -> bool {
        self.slots
            .get(&slot)
            .and_then(|s| s.ready.as_ref())
            .is_some_and(|(k, _)| *k == key)
    }

    /// Whether any bake is running.
    pub fn pending(&self) -> bool {
        self.slots.values().any(|s| s.work.is_some())
    }

    /// Progress of the running bakes (0..1, the slowest), `None` if none.
    pub fn progress(&self) -> Option<f32> {
        self.slots
            .values()
            .filter_map(|s| s.work.as_ref())
            .map(|w| match &w.run {
                Run::Inline(job) => job.progress(),
                #[cfg(not(target_arch = "wasm32"))]
                Run::Thread { progress, .. } => {
                    f32::from_bits(progress.load(std::sync::atomic::Ordering::Relaxed))
                }
            })
            .reduce(f32::min)
    }

    /// Call every frame: collects finished bakes, and runs inline bakes
    /// for as long as `more()` says there is time (check a clock in it).
    pub fn poll(&mut self, mut more: impl FnMut() -> bool) {
        for s in self.slots.values_mut() {
            let Some(work) = s.work.as_mut() else {
                continue;
            };
            let outcome = match &mut work.run {
                Run::Inline(job) => {
                    let mut done = false;
                    while !done && more() {
                        done = job.step(CHUNK);
                    }
                    if done {
                        Outcome::Finish
                    } else {
                        Outcome::Running
                    }
                }
                #[cfg(not(target_arch = "wasm32"))]
                Run::Thread { rx, .. } => match rx.try_recv() {
                    Ok(bake) => Outcome::Ready(bake),
                    Err(std::sync::mpsc::TryRecvError::Empty) => Outcome::Running,
                    // The thread panicked: forget the bake (asking again
                    // restarts it).
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => Outcome::Lost,
                },
            };
            outcome.apply(s);
        }
    }

    /// Waits for every running bake (exports, which must be exact).
    pub fn finish_all(&mut self) {
        for s in self.slots.values_mut() {
            let outcome = match s.work.as_ref().map(|w| &w.run) {
                None => continue,
                Some(Run::Inline(_)) => Outcome::Finish,
                #[cfg(not(target_arch = "wasm32"))]
                Some(Run::Thread { rx, .. }) => rx.recv().map_or(Outcome::Lost, Outcome::Ready),
            };
            outcome.apply(s);
        }
    }

    /// Call once per frame after the `get`s: forgets the slots nobody asked
    /// for since the last call (deleted layers, simulations turned off).
    pub fn end_frame(&mut self) {
        self.slots.retain(|_, s| std::mem::take(&mut s.used));
    }
}

/// What became of a slot's running bake.
enum Outcome {
    Running,
    /// An inline bake to run to the end.
    Finish,
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    Ready(Bake),
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    Lost,
}

impl Outcome {
    fn apply(self, s: &mut Slot) {
        match self {
            Outcome::Running => {}
            Outcome::Finish => {
                if let Some(Work {
                    key,
                    run: Run::Inline(job),
                }) = s.work.take()
                {
                    s.ready = Some((key, Arc::new(job.finish())));
                }
            }
            Outcome::Ready(bake) => {
                if let Some(w) = s.work.take() {
                    s.ready = Some((w.key, Arc::new(bake)));
                }
            }
            Outcome::Lost => s.work = None,
        }
    }
}

#[cfg(target_arch = "wasm32")]
fn start(job: BakeJob, _threads: bool) -> Run {
    Run::Inline(Box::new(job))
}

#[cfg(not(target_arch = "wasm32"))]
fn start(mut job: BakeJob, threads: bool) -> Run {
    use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
    if !threads {
        return Run::Inline(Box::new(job));
    }
    let (tx, rx) = std::sync::mpsc::channel();
    let progress = Arc::new(AtomicU32::new(0));
    let cancel = Arc::new(AtomicBool::new(false));
    let (p, c) = (progress.clone(), cancel.clone());
    std::thread::spawn(move || {
        while !job.step(CHUNK) {
            if c.load(Ordering::Relaxed) {
                return;
            }
            p.store(job.progress().to_bits(), Ordering::Relaxed);
        }
        let _ = tx.send(job.finish());
    });
    Run::Thread {
        rx,
        progress,
        _cancel: Cancel(cancel),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::{Body, Sim, SimClock, SimLoop};
    use crate::{EvalCtx, Timing};

    /// Bodies drifting at their own speed.
    struct Drift(Vec<Body>);

    impl Sim for Drift {
        fn bodies(&self) -> &[Body] {
            &self.0
        }
        fn bodies_mut(&mut self) -> &mut [Body] {
            &mut self.0
        }
        fn step(&mut self, dt: f32, _: &EvalCtx) {
            for b in &mut self.0 {
                b.pos += b.vel * dt;
            }
        }
    }

    fn job(speed: f32) -> BakeJob {
        let body = Body {
            vel: glam::Vec3::X * speed,
            ..Body::default()
        };
        BakeJob::new(
            Box::new(Drift(vec![body; 3])),
            SimClock::new(Timing::default()),
            SimLoop::default(),
        )
    }

    fn speed(bake: &Bake) -> f32 {
        let mut f = crate::sim::Frame::default();
        bake.sample(0.1, &mut f);
        f.layers[0].bodies[0].vel.x
    }

    #[test]
    fn inline_bakes_run_in_slices_and_keep_the_old_one() {
        let mut cache = BakeCache::inline();
        assert!(cache.get(1, 10, || job(1.0)).is_none());
        assert!(cache.pending() && !cache.is_ready(1, 10));
        // A few slices, not the whole bake.
        let mut slices = 3;
        cache.poll(|| {
            slices -= 1;
            slices >= 0
        });
        let p = cache.progress().unwrap();
        assert!(p > 0.0 && p < 1.0, "{p}");
        cache.poll(|| true);
        assert!(!cache.pending() && cache.is_ready(1, 10));
        assert_eq!(speed(&cache.get(1, 10, || unreachable!()).unwrap()), 1.0);

        // New settings: the old bake stays until the new one is done.
        let old = cache.get(1, 11, || job(2.0)).unwrap();
        assert_eq!(speed(&old), 1.0);
        assert!(cache.pending() && !cache.is_ready(1, 11));
        // Asking again doesn't restart it.
        cache.get(1, 11, || unreachable!());
        cache.finish_all();
        assert_eq!(speed(&cache.get(1, 11, || unreachable!()).unwrap()), 2.0);
    }

    #[test]
    fn threads_bake_in_the_background() {
        let mut cache = BakeCache::new();
        cache.get(1, 1, || job(1.0));
        cache.get(2, 2, || job(3.0));
        cache.finish_all();
        assert!(!cache.pending());
        assert_eq!(speed(&cache.get(1, 1, || unreachable!()).unwrap()), 1.0);
        assert_eq!(speed(&cache.get(2, 2, || unreachable!()).unwrap()), 3.0);
        // Polling picks up a finished bake too.
        cache.get(1, 5, || job(5.0));
        let start = std::time::Instant::now();
        while !cache.is_ready(1, 5) {
            assert!(start.elapsed().as_secs() < 30);
            cache.poll(|| false);
            std::thread::yield_now();
        }
        assert_eq!(speed(&cache.get(1, 5, || unreachable!()).unwrap()), 5.0);
    }

    #[test]
    fn unused_slots_are_forgotten() {
        let mut cache = BakeCache::inline();
        cache.get(1, 1, || job(1.0));
        cache.get(2, 2, || job(1.0));
        cache.finish_all();
        cache.end_frame();
        // Only slot 1 is still asked for.
        cache.get(1, 1, || unreachable!());
        cache.end_frame();
        cache.end_frame();
        assert!(!cache.is_ready(2, 2));
        let mut restarted = false;
        cache.get(2, 2, || {
            restarted = true;
            job(1.0)
        });
        assert!(restarted);
    }
}
