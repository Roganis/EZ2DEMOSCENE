//! Bakes without freezing the editor: on desktop each runs on a thread; in
//! the browser (no threads) they run a slice at a time from
//! [`BakeCache::poll`].

use super::{Bake, BakeJob};
use std::collections::HashMap;
use std::sync::Arc;

/// Simulation steps between checks of the time budget (or cancelling).
const CHUNK: usize = 64;

/// Bakes by key (a hash of everything a bake depends on, see
/// [`KeyHasher`](super::KeyHasher)), asked for through a slot (one per
/// simulated layer, numbered by the caller). While a slot's new bake runs,
/// it keeps handing out the bake it showed last, so the preview doesn't
/// blank out while a setting is dragged. Bakes nobody asked for in a while
/// are forgotten (see [`BakeCache::end_frame`]).
pub struct BakeCache {
    threads: bool,
    /// Frames a bake is kept without being asked for.
    keep: u64,
    frame: u64,
    ready: HashMap<u64, Ready>,
    work: HashMap<u64, Work>,
    /// The key each slot showed last, and when it was asked for.
    slots: HashMap<u64, (u64, u64)>,
}

struct Ready {
    bake: Arc<Bake>,
    used: u64,
}

struct Work {
    run: Run,
    used: u64,
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
            keep: 240,
            frame: 0,
            ready: HashMap::new(),
            work: HashMap::new(),
            slots: HashMap::new(),
        }
    }

    /// Bakes only inside [`BakeCache::poll`] and [`BakeCache::finish_all`].
    pub fn inline() -> BakeCache {
        BakeCache {
            threads: false,
            ..Self::new()
        }
    }

    /// Keep bakes for `frames` calls of [`BakeCache::end_frame`] after
    /// they were last asked for (at least 1).
    pub fn keep_for(mut self, frames: u64) -> BakeCache {
        self.keep = frames.max(1);
        self
    }

    /// The bake for `key`, starting it with `job` if it isn't there or on
    /// its way. Until it is ready, the bake `slot` showed last (if any);
    /// [`BakeCache::is_ready`] tells which.
    pub fn get(&mut self, slot: u64, key: u64, job: impl FnOnce() -> BakeJob) -> Option<Arc<Bake>> {
        let frame = self.frame;
        if let Some(r) = self.ready.get_mut(&key) {
            r.used = frame;
            self.slots.insert(slot, (key, frame));
            return Some(r.bake.clone());
        }
        match self.work.get_mut(&key) {
            Some(w) => w.used = frame,
            None => {
                let run = start(job(), self.threads);
                self.work.insert(key, Work { run, used: frame });
            }
        }
        let shown = self.slots.get_mut(&slot)?;
        shown.1 = frame;
        let r = self.ready.get_mut(&shown.0)?;
        r.used = frame;
        Some(r.bake.clone())
    }

    /// Whether the bake for `key` is ready.
    pub fn is_ready(&self, key: u64) -> bool {
        self.ready.contains_key(&key)
    }

    /// Whether any bake is running.
    pub fn pending(&self) -> bool {
        !self.work.is_empty()
    }

    /// Progress of the running bakes (0..1, the slowest), `None` if none.
    pub fn progress(&self) -> Option<f32> {
        self.work
            .values()
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
        let mut keys: Vec<u64> = self.work.keys().copied().collect();
        // Same order every time (inline bakes share the time).
        keys.sort_unstable();
        for key in keys {
            let Some(w) = self.work.get_mut(&key) else {
                continue;
            };
            let outcome = match &mut w.run {
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
            self.apply(key, outcome);
        }
    }

    /// Waits for every running bake (exports, which must be exact).
    pub fn finish_all(&mut self) {
        let keys: Vec<u64> = self.work.keys().copied().collect();
        for key in keys {
            let outcome = match self.work.get(&key).map(|w| &w.run) {
                None => continue,
                Some(Run::Inline(_)) => Outcome::Finish,
                #[cfg(not(target_arch = "wasm32"))]
                Some(Run::Thread { rx, .. }) => rx.recv().map_or(Outcome::Lost, Outcome::Ready),
            };
            self.apply(key, outcome);
        }
    }

    fn apply(&mut self, key: u64, outcome: Outcome) {
        let used = match outcome {
            Outcome::Running => return,
            _ => self.work.get(&key).map_or(self.frame, |w| w.used),
        };
        let bake = match outcome {
            Outcome::Running => return,
            Outcome::Lost => {
                self.work.remove(&key);
                return;
            }
            Outcome::Ready(bake) => bake,
            Outcome::Finish => match self.work.remove(&key).map(|w| w.run) {
                Some(Run::Inline(job)) => job.finish(),
                _ => return,
            },
        };
        self.work.remove(&key);
        self.ready.insert(
            key,
            Ready {
                bake: Arc::new(bake),
                used,
            },
        );
    }

    /// Call once per frame: forgets bakes (and stops running ones) nobody
    /// asked for in the last frames (deleted layers, old settings).
    pub fn end_frame(&mut self) {
        self.frame += 1;
        let (now, keep) = (self.frame, self.keep);
        let fresh = |used: u64| now - used <= keep;
        self.ready.retain(|_, r| fresh(r.used));
        self.work.retain(|_, w| fresh(w.used));
        self.slots.retain(|_, (_, used)| fresh(*used));
    }
}

/// What became of a running bake.
enum Outcome {
    Running,
    /// An inline bake to run to the end.
    Finish,
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    Ready(Bake),
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    Lost,
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
        assert!(cache.pending() && !cache.is_ready(10));
        // A few slices, not the whole bake.
        let mut slices = 3;
        cache.poll(|| {
            slices -= 1;
            slices >= 0
        });
        let p = cache.progress().unwrap();
        assert!(p > 0.0 && p < 1.0, "{p}");
        cache.poll(|| true);
        assert!(!cache.pending() && cache.is_ready(10));
        assert_eq!(speed(&cache.get(1, 10, || unreachable!()).unwrap()), 1.0);

        // New settings: the old bake stays until the new one is done.
        let old = cache.get(1, 11, || job(2.0)).unwrap();
        assert_eq!(speed(&old), 1.0);
        assert!(cache.pending() && !cache.is_ready(11));
        // Asking again doesn't restart it.
        cache.get(1, 11, || unreachable!());
        cache.finish_all();
        assert_eq!(speed(&cache.get(1, 11, || unreachable!()).unwrap()), 2.0);
        // Both are kept while asked for: going back is instant.
        assert_eq!(speed(&cache.get(1, 10, || unreachable!()).unwrap()), 1.0);
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
        while !cache.is_ready(5) {
            assert!(start.elapsed().as_secs() < 30);
            cache.poll(|| false);
            std::thread::yield_now();
        }
        assert_eq!(speed(&cache.get(1, 5, || unreachable!()).unwrap()), 5.0);
    }

    #[test]
    fn two_slots_can_share_or_differ() {
        // Two scenes during a transition: neither restarts the other.
        let mut cache = BakeCache::inline();
        cache.get(1, 1, || job(1.0));
        cache.get(2, 2, || job(2.0));
        cache.finish_all();
        for _ in 0..3 {
            assert_eq!(speed(&cache.get(1, 1, || unreachable!()).unwrap()), 1.0);
            assert_eq!(speed(&cache.get(2, 2, || unreachable!()).unwrap()), 2.0);
            assert_eq!(speed(&cache.get(3, 1, || unreachable!()).unwrap()), 1.0);
            cache.end_frame();
        }
    }

    #[test]
    fn unused_bakes_are_forgotten() {
        let mut cache = BakeCache::inline().keep_for(2);
        cache.get(1, 1, || job(1.0));
        cache.get(2, 2, || job(1.0));
        cache.finish_all();
        // Only slot 1 is still asked for.
        for _ in 0..3 {
            cache.get(1, 1, || unreachable!());
            cache.end_frame();
        }
        assert!(cache.is_ready(1) && !cache.is_ready(2));
        let mut restarted = false;
        cache.get(2, 2, || {
            restarted = true;
            job(1.0)
        });
        assert!(restarted);
    }
}
