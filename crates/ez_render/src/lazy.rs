//! GPU objects built the first time they are used.
//!
//! Compiling every pipeline up front took seconds at start-up on some
//! drivers, and most scenes only use a few of them. A [`Lazy`] holds what
//! it takes to build its object and builds it when first dereferenced.

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

    /// The object, built now if it wasn't yet.
    pub fn get(&self) -> &T {
        self.value.get_or_init(|| (self.make)())
    }

    /// Whether the object has been built.
    #[cfg(test)]
    pub fn is_built(&self) -> bool {
        self.value.get().is_some()
    }
}

impl<T> std::ops::Deref for Lazy<T> {
    type Target = T;
    fn deref(&self) -> &T {
        self.get()
    }
}

/// A render pipeline built when first drawn with.
pub type Pipe = Lazy<wgpu::RenderPipeline>;

/// A shader module (shared by several pipelines), parsed when the first
/// of them is built.
pub type Module = Arc<Lazy<wgpu::ShaderModule>>;

/// `lazy!(device, a, b => expr)`: a [`Lazy`] evaluating `expr` when first
/// used, with its own clones of `device` (seen as `&wgpu::Device` in
/// `expr`, like the constructor's) and of `a`, `b` (wgpu handles are cheap
/// to clone).
macro_rules! lazy {
    ($device:ident $(, $v:ident)* => $body:expr) => {{
        let $device = $device.clone();
        $(
            #[allow(clippy::redundant_clone)]
            let $v = $v.clone();
        )*
        $crate::lazy::Lazy::new(move || {
            let $device = &$device;
            $body
        })
    }};
}
pub(crate) use lazy;

#[cfg(test)]
mod tests {
    use super::Lazy;
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
}
