//! Live music input (microphone / line-in) for the preview. Live input is
//! not repeatable, so exports always use the analysed music file instead.

use anyhow::Result;
use ez_core::analysis::LiveAnalyzer;
use ez_core::MusicFrame;

#[cfg(not(target_arch = "wasm32"))]
pub use desktop::LiveInput;
#[cfg(target_arch = "wasm32")]
pub use web::LiveInput;

#[cfg(not(target_arch = "wasm32"))]
mod desktop {
    use super::*;
    use anyhow::Context;
    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
    use std::sync::{Arc, Mutex};

    pub struct LiveInput {
        analyzer: Arc<Mutex<LiveAnalyzer>>,
        _stream: cpal::Stream,
        pub name: String,
    }

    impl LiveInput {
        pub fn start() -> Result<LiveInput> {
            let host = cpal::default_host();
            let device = host
                .default_input_device()
                .context("no microphone or line-in found")?;
            let name = device
                .description()
                .map(|d| d.name().to_string())
                .unwrap_or_else(|_| "input".into());
            let supported = device
                .default_input_config()
                .context("the input device has no usable format")?;
            let channels = supported.channels().max(1) as usize;
            let rate = supported.sample_rate() as f32;
            let format = supported.sample_format();
            let config: cpal::StreamConfig = supported.into();
            let analyzer = Arc::new(Mutex::new(LiveAnalyzer::new(rate)));
            let a = analyzer.clone();
            let err = |e| log::warn!("live input: {e}");
            fn mono<T: Copy>(data: &[T], ch: usize, f: impl Fn(T) -> f32) -> Vec<f32> {
                data.chunks(ch)
                    .map(|c| c.iter().map(|s| f(*s)).sum::<f32>() / ch as f32)
                    .collect()
            }
            let stream = match format {
                cpal::SampleFormat::F32 => device.build_input_stream(
                    &config,
                    move |d: &[f32], _: &_| {
                        if let Ok(mut a) = a.lock() {
                            a.push(&mono(d, channels, |s| s));
                        }
                    },
                    err,
                    None,
                ),
                cpal::SampleFormat::I16 => device.build_input_stream(
                    &config,
                    move |d: &[i16], _: &_| {
                        if let Ok(mut a) = a.lock() {
                            a.push(&mono(d, channels, |s| s as f32 / 32768.0));
                        }
                    },
                    err,
                    None,
                ),
                cpal::SampleFormat::U16 => device.build_input_stream(
                    &config,
                    move |d: &[u16], _: &_| {
                        if let Ok(mut a) = a.lock() {
                            a.push(&mono(d, channels, |s| (s as f32 - 32768.0) / 32768.0));
                        }
                    },
                    err,
                    None,
                ),
                other => anyhow::bail!("unsupported input format {other:?}"),
            }
            .context("opening the input")?;
            stream.play().context("starting the input")?;
            Ok(LiveInput {
                analyzer,
                _stream: stream,
                name,
            })
        }

        pub fn frame(&mut self, _dt: f32) -> MusicFrame {
            self.analyzer.lock().map(|a| a.frame()).unwrap_or_default()
        }
    }
}

#[cfg(target_arch = "wasm32")]
mod web {
    use super::*;
    use anyhow::anyhow;
    use std::cell::RefCell;
    use std::rc::Rc;
    use wasm_bindgen::JsCast;

    pub struct LiveInput {
        ctx: web_sys::AudioContext,
        node: Rc<RefCell<Option<web_sys::AnalyserNode>>>,
        error: Rc<RefCell<Option<String>>>,
        analyzer: LiveAnalyzer,
        buf: Vec<f32>,
        pub name: String,
    }

    impl LiveInput {
        /// Asks the browser for the microphone; the analysis starts once
        /// permission is granted.
        pub fn start() -> Result<LiveInput> {
            let ctx = web_sys::AudioContext::new().map_err(|e| anyhow!("{e:?}"))?;
            let rate = ctx.sample_rate();
            let window = web_sys::window().ok_or_else(|| anyhow!("no window"))?;
            let devices = window
                .navigator()
                .media_devices()
                .map_err(|_| anyhow!("this browser has no microphone access"))?;
            let c = web_sys::MediaStreamConstraints::new();
            c.set_audio(&wasm_bindgen::JsValue::TRUE);
            let promise = devices
                .get_user_media_with_constraints(&c)
                .map_err(|e| anyhow!("{e:?}"))?;
            let node = Rc::new(RefCell::new(None));
            let error = Rc::new(RefCell::new(None));
            let (n2, e2, ctx2) = (node.clone(), error.clone(), ctx.clone());
            wasm_bindgen_futures::spawn_local(async move {
                match wasm_bindgen_futures::JsFuture::from(promise).await {
                    Ok(stream) => {
                        let stream: web_sys::MediaStream = stream.unchecked_into();
                        let result =
                            (|| -> Result<web_sys::AnalyserNode, wasm_bindgen::JsValue> {
                                let src = ctx2.create_media_stream_source(&stream)?;
                                let an = ctx2.create_analyser()?;
                                an.set_fft_size(2048);
                                src.connect_with_audio_node(&an)?;
                                let _ = ctx2.resume();
                                Ok(an)
                            })();
                        match result {
                            Ok(an) => *n2.borrow_mut() = Some(an),
                            Err(e) => *e2.borrow_mut() = Some(format!("{e:?}")),
                        }
                    }
                    Err(_) => *e2.borrow_mut() = Some("microphone access was denied".into()),
                }
            });
            Ok(LiveInput {
                ctx,
                node,
                error,
                analyzer: LiveAnalyzer::new(rate),
                buf: vec![0.0; 2048],
                name: "microphone".into(),
            })
        }

        /// A problem reported by the browser, if any.
        pub fn error(&self) -> Option<String> {
            self.error.borrow().clone()
        }

        pub fn frame(&mut self, dt: f32) -> MusicFrame {
            if let Some(an) = self.node.borrow().as_ref() {
                an.get_float_time_domain_data(&mut self.buf);
                // The analyser only shows the latest 2048 samples: feed the
                // part that is new since the last frame.
                let new = ((self.ctx.sample_rate() * dt) as usize).clamp(1, self.buf.len());
                self.analyzer.push(&self.buf[self.buf.len() - new..]);
            }
            self.analyzer.frame()
        }
    }

    impl Drop for LiveInput {
        fn drop(&mut self) {
            let _ = self.ctx.close();
        }
    }
}
