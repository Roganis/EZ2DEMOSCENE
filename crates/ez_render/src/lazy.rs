//! GPU objects built the first time they are used.
//!
//! Compiling every pipeline up front took seconds at start-up on some
//! drivers, and most scenes only use a few of them. A [`Lazy`] holds what
//! it takes to build its object and builds it when first dereferenced.
//!
//! The editor then builds the rest on a thread ([`WarmList::spawn`]), so
//! the first use of an effect doesn't stall a frame either. Objects made
//! with [`lazy!`] while a [`Collect`] is running are listed for it.

use std::sync::{Arc, OnceLock};

/// Builds the object of a [`Lazy`].
#[cfg(not(target_arch = "wasm32"))]
pub trait Make<T>: Fn() -> T + Send + Sync + 'static {}
#[cfg(not(target_arch = "wasm32"))]
impl<T, F: Fn() -> T + Send + Sync + 'static> Make<T> for F {}

/// Builds the object of a [`Lazy`] (wgpu objects aren't `Send` on the web).
#[cfg(target_arch = "wasm32")]
pub trait Make<T>: Fn() -> T + 'static {}
#[cfg(target_arch = "wasm32")]
impl<T, F: Fn() -> T + 'static> Make<T> for F {}

pub struct Lazy<T> {
    value: OnceLock<T>,
    make: Box<dyn Make<T>>,
}

impl<T> Lazy<T> {
    pub fn new(make: impl Make<T>) -> Lazy<T> {
        Lazy {
            value: OnceLock::new(),
            make: Box::new(make),
        }
    }

    /// The object, built now if it wasn't yet (or waiting while the
    /// warm-up thread builds it).
    pub fn get(&self) -> &T {
        self.value.get_or_init(|| (self.make)())
    }

    /// Whether the object has been built.
    #[cfg(test)]
    pub fn is_built(&self) -> bool {
        self.value.get().is_some()
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl<T: Send + Sync + 'static> Lazy<T> {
    /// A shared [`Lazy`], listed for the warm-up if a [`Collect`] runs.
    pub fn shared(make: impl Make<T>) -> Arc<Lazy<T>> {
        let lazy = Arc::new(Lazy::new(make));
        let weak: std::sync::Weak<dyn Warm> = Arc::downgrade(&lazy) as _;
        COLLECT.with(|c| {
            if let Some(list) = c.borrow_mut().as_mut() {
                list.push(weak);
            }
        });
        lazy
    }
}

#[cfg(target_arch = "wasm32")]
impl<T: 'static> Lazy<T> {
    /// A shared [`Lazy`] (no warm-up without threads).
    pub fn shared(make: impl Make<T>) -> Arc<Lazy<T>> {
        Arc::new(Lazy::new(make))
    }
}

impl<T> std::ops::Deref for Lazy<T> {
    type Target = T;
    fn deref(&self) -> &T {
        self.get()
    }
}

/// A render pipeline built when first drawn with.
pub type Pipe = Arc<Lazy<wgpu::RenderPipeline>>;

/// A shader module (shared by several pipelines), parsed when the first
/// of them is built.
pub type Module = Arc<Lazy<wgpu::ShaderModule>>;

/// Something the warm-up can build ahead of use.
#[cfg(not(target_arch = "wasm32"))]
trait Warm: Send + Sync {
    fn warm(&self);
}

#[cfg(not(target_arch = "wasm32"))]
impl<T: Send + Sync> Warm for Lazy<T> {
    fn warm(&self) {
        self.get();
    }
}

#[cfg(not(target_arch = "wasm32"))]
thread_local! {
    static COLLECT: std::cell::RefCell<Option<Vec<std::sync::Weak<dyn Warm>>>> =
        const { std::cell::RefCell::new(None) };
}

/// Lists the shared [`Lazy`]s made on this thread from [`Collect::start`]
/// to [`Collect::finish`].
pub struct Collect(());

impl Collect {
    pub fn start() -> Collect {
        #[cfg(not(target_arch = "wasm32"))]
        COLLECT.with(|c| *c.borrow_mut() = Some(Vec::new()));
        Collect(())
    }

    pub fn finish(self) -> WarmList {
        #[cfg(not(target_arch = "wasm32"))]
        {
            WarmList {
                items: COLLECT
                    .with(|c| c.borrow_mut().take())
                    .unwrap_or_default()
                    .into(),
                done: None,
            }
        }
        #[cfg(target_arch = "wasm32")]
        WarmList::default()
    }
}

impl Drop for Collect {
    fn drop(&mut self) {
        #[cfg(not(target_arch = "wasm32"))]
        COLLECT.with(|c| *c.borrow_mut() = None);
    }
}

/// Called once everything is built.
type Done = Box<dyn FnOnce() + Send>;

/// The objects to build ahead of use, in the order they were made. Only
/// weakly held: the warm-up stops with their owner. (Empty in the
/// browser: there is no thread to build them on.)
#[derive(Default)]
pub struct WarmList {
    #[cfg(not(target_arch = "wasm32"))]
    items: std::collections::VecDeque<std::sync::Weak<dyn Warm>>,
    /// Set while [`WarmList::step`] builds them.
    done: Option<Done>,
}

impl WarmList {
    /// Build everything on a thread, then call `done` (on that thread).
    /// Empties the list. Where there are no threads, `done` is called at
    /// once.
    pub fn spawn(&mut self, done: impl FnOnce() + Send + 'static) {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let list = std::mem::take(&mut self.items);
            let spawned = std::thread::Builder::new()
                .name("ez2 pipeline warm-up".into())
                .spawn(move || {
                    let start = std::time::Instant::now();
                    let mut built = 0;
                    for item in list {
                        if let Some(item) = item.upgrade() {
                            item.warm();
                            built += 1;
                        }
                    }
                    log::info!("warmed up {built} GPU objects in {:?}", start.elapsed());
                    done();
                });
            if let Err(e) = spawned {
                log::warn!("pipeline warm-up: {e}");
            }
        }
        #[cfg(target_arch = "wasm32")]
        done();
    }

    /// Build them on this thread instead, one per [`WarmList::step`];
    /// the step building the last calls `done`.
    pub fn start_steps(&mut self, done: impl FnOnce() + Send + 'static) {
        self.done = Some(Box::new(done));
        self.step();
    }

    /// Build the next object if [`WarmList::start_steps`] was called.
    /// Returns whether there is more to build.
    pub fn step(&mut self) -> bool {
        if self.done.is_none() {
            return false;
        }
        #[cfg(not(target_arch = "wasm32"))]
        while let Some(item) = self.items.pop_front() {
            if let Some(item) = item.upgrade() {
                item.warm();
                break;
            }
        }
        #[cfg(not(target_arch = "wasm32"))]
        if !self.items.is_empty() {
            return true;
        }
        if let Some(done) = self.done.take() {
            log::info!("warmed up GPU objects a frame at a time");
            done();
        }
        false
    }
}

/// `lazy!(device, a, b => expr)`: a shared [`Lazy`] evaluating `expr` when
/// first used, with its own clones of `device` (seen as `&wgpu::Device` in
/// `expr`, like the constructor's) and of `a`, `b` (wgpu handles are cheap
/// to clone).
macro_rules! lazy {
    ($device:ident $(, $v:ident)* => $body:expr) => {{
        let $device = $device.clone();
        $(
            #[allow(clippy::redundant_clone)]
            let $v = $v.clone();
        )*
        $crate::lazy::Lazy::shared(move || {
            let $device = &$device;
            $body
        })
    }};
}
pub(crate) use lazy;

#[cfg(test)]
mod tests {
    use super::{Collect, Lazy};
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;

    #[test]
    fn builds_once_on_first_use() {
        let calls = Arc::new(AtomicU32::new(0));
        let c = calls.clone();
        let lazy = Lazy::new(move || {
            c.fetch_add(1, Ordering::Relaxed);
            7
        });
        assert!(!lazy.is_built());
        assert_eq!(calls.load(Ordering::Relaxed), 0);
        assert_eq!(*lazy + *lazy, 14);
        assert!(lazy.is_built());
        assert_eq!(calls.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn warm_up_builds_what_was_collected_and_skips_dropped_ones() {
        let collect = Collect::start();
        let kept = Lazy::shared(|| 1);
        let dropped = Lazy::shared(|| -> i32 { panic!("dropped before the warm-up") });
        let mut list = collect.finish();
        // Made after the collection: not listed.
        let late = Lazy::shared(|| 3);
        drop(dropped);
        let (tx, rx) = std::sync::mpsc::channel();
        list.spawn(move || tx.send(()).unwrap());
        rx.recv_timeout(std::time::Duration::from_secs(10))
            .expect("warm-up finished");
        assert!(kept.is_built());
        assert!(!late.is_built());
    }

    #[test]
    fn steps_build_one_at_a_time() {
        let collect = Collect::start();
        let a = Lazy::shared(|| 1);
        let b = Lazy::shared(|| 2);
        let mut list = collect.finish();
        assert!(!list.step(), "nothing before start_steps");
        let done = Arc::new(AtomicU32::new(0));
        let d = done.clone();
        list.start_steps(move || {
            d.fetch_add(1, Ordering::Relaxed);
        });
        assert!(a.is_built() && !b.is_built());
        assert!(!list.step());
        assert!(b.is_built());
        assert_eq!(done.load(Ordering::Relaxed), 1);
        assert!(!list.step());
        assert_eq!(done.load(Ordering::Relaxed), 1);
    }
}
