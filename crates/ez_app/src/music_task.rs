//! Music loading without freezing the editor: on desktop the analysis runs
//! on a thread; in the browser (no threads) it runs a slice per frame.

use anyhow::Result;
use ez_export::{LoadedMusic, MusicJob};

#[allow(clippy::large_enum_variant)] // one short-lived value
pub enum Started {
    /// Nothing slow to do (no audio file, or its analysis was cached).
    Done(Result<LoadedMusic>),
    Working(MusicTask),
}

pub struct MusicTask {
    #[cfg(not(target_arch = "wasm32"))]
    rx: std::sync::mpsc::Receiver<Result<LoadedMusic>>,
    #[cfg(not(target_arch = "wasm32"))]
    progress: std::sync::Arc<std::sync::atomic::AtomicU32>,
    #[cfg(target_arch = "wasm32")]
    job: Option<MusicJob>,
}

impl MusicTask {
    pub fn start(mut job: MusicJob) -> Started {
        match job.step(0) {
            Ok(true) => return Started::Done(job.run()),
            Ok(false) => {}
            Err(e) => return Started::Done(Err(e)),
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            use std::sync::atomic::Ordering;
            let (tx, rx) = std::sync::mpsc::channel();
            let progress = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
            let p = progress.clone();
            std::thread::spawn(move || {
                let res = (|| {
                    while !job.step(200)? {
                        p.store(job.progress().to_bits(), Ordering::Relaxed);
                    }
                    job.run()
                })();
                let _ = tx.send(res);
            });
            Started::Working(MusicTask { rx, progress })
        }
        #[cfg(target_arch = "wasm32")]
        Started::Working(MusicTask { job: Some(job) })
    }

    /// 0..1.
    pub fn progress(&self) -> f32 {
        #[cfg(not(target_arch = "wasm32"))]
        return f32::from_bits(self.progress.load(std::sync::atomic::Ordering::Relaxed));
        #[cfg(target_arch = "wasm32")]
        self.job.as_ref().map_or(1.0, |j| j.progress())
    }

    /// Call every frame; the result once it is ready.
    pub fn poll(&mut self) -> Option<Result<LoadedMusic>> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            use std::sync::mpsc::TryRecvError;
            match self.rx.try_recv() {
                Ok(r) => Some(r),
                Err(TryRecvError::Empty) => None,
                Err(TryRecvError::Disconnected) => {
                    Some(Err(anyhow::anyhow!("music analysis stopped unexpectedly")))
                }
            }
        }
        #[cfg(target_arch = "wasm32")]
        {
            // About 10 ms of work per frame keeps the page responsive.
            let start = js_sys::Date::now();
            let job = self.job.as_mut()?;
            loop {
                match job.step(20) {
                    Ok(true) => return self.job.take().map(|j| j.run()),
                    Ok(false) => {}
                    Err(e) => {
                        self.job = None;
                        return Some(Err(e));
                    }
                }
                if js_sys::Date::now() - start > 10.0 {
                    return None;
                }
            }
        }
    }
}
