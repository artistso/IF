use inkframe_core::StrokeSample;
use std::fmt;
use std::sync::mpsc::{self, Receiver, SyncSender, TrySendError};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

pub const DEFAULT_QUEUE_CAPACITY: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NativeSurface {
    /// Platform handle encoded as an integer. The platform renderer owns the referenced object.
    pub handle: usize,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug)]
pub enum EngineCommand {
    AttachSurface(NativeSurface),
    DetachSurface,
    Resize { width: u32, height: u32 },
    Input(Vec<StrokeSample>),
    Shutdown,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct EngineStats {
    pub input_packets: u64,
    pub input_samples: u64,
    pub dropped_packets: u64,
    pub renderer_errors: u64,
}

pub trait RendererBackend: Send + 'static {
    fn attach_surface(&mut self, surface: NativeSurface) -> Result<(), String>;
    fn detach_surface(&mut self);
    fn resize(&mut self, width: u32, height: u32) -> Result<(), String>;
    fn ingest_input(&mut self, samples: &[StrokeSample]) -> Result<(), String>;
}

#[derive(Default)]
pub struct NullRenderer;

impl RendererBackend for NullRenderer {
    fn attach_surface(&mut self, _surface: NativeSurface) -> Result<(), String> { Ok(()) }
    fn detach_surface(&mut self) {}
    fn resize(&mut self, _width: u32, _height: u32) -> Result<(), String> { Ok(()) }
    fn ingest_input(&mut self, _samples: &[StrokeSample]) -> Result<(), String> { Ok(()) }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubmitError {
    Backpressure,
    Stopped,
    InvalidSurfaceSize,
}

impl fmt::Display for SubmitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Backpressure => write!(f, "engine command queue is full"),
            Self::Stopped => write!(f, "engine thread has stopped"),
            Self::InvalidSurfaceSize => write!(f, "surface dimensions must be non-zero"),
        }
    }
}

impl std::error::Error for SubmitError {}

pub struct EngineHost {
    sender: SyncSender<EngineCommand>,
    stats: Arc<Mutex<EngineStats>>,
    thread: Mutex<Option<JoinHandle<()>>>,
}

impl EngineHost {
    pub fn spawn(renderer: impl RendererBackend) -> Self {
        Self::spawn_with_capacity(renderer, DEFAULT_QUEUE_CAPACITY)
    }

    pub fn spawn_with_capacity(renderer: impl RendererBackend, capacity: usize) -> Self {
        let (sender, receiver) = mpsc::sync_channel(capacity.max(1));
        let stats = Arc::new(Mutex::new(EngineStats::default()));
        let thread_stats = Arc::clone(&stats);
        let thread = thread::Builder::new()
            .name("inkframe-engine".into())
            .spawn(move || run_engine(receiver, renderer, thread_stats))
            .expect("failed to spawn InkFrame engine thread");
        Self {
            sender,
            stats,
            thread: Mutex::new(Some(thread)),
        }
    }

    pub fn attach_surface(&self, surface: NativeSurface) -> Result<(), SubmitError> {
        if surface.width == 0 || surface.height == 0 {
            return Err(SubmitError::InvalidSurfaceSize);
        }
        self.send(EngineCommand::AttachSurface(surface))
    }

    pub fn detach_surface(&self) -> Result<(), SubmitError> {
        self.send(EngineCommand::DetachSurface)
    }

    pub fn resize(&self, width: u32, height: u32) -> Result<(), SubmitError> {
        if width == 0 || height == 0 {
            return Err(SubmitError::InvalidSurfaceSize);
        }
        self.send(EngineCommand::Resize { width, height })
    }

    pub fn submit_input(&self, samples: Vec<StrokeSample>) -> Result<(), SubmitError> {
        if samples.is_empty() {
            return Ok(());
        }
        match self.sender.try_send(EngineCommand::Input(samples)) {
            Ok(()) => Ok(()),
            Err(TrySendError::Full(_)) => {
                self.stats.lock().unwrap().dropped_packets += 1;
                Err(SubmitError::Backpressure)
            }
            Err(TrySendError::Disconnected(_)) => Err(SubmitError::Stopped),
        }
    }

    pub fn stats(&self) -> EngineStats {
        *self.stats.lock().unwrap()
    }

    fn send(&self, command: EngineCommand) -> Result<(), SubmitError> {
        match self.sender.try_send(command) {
            Ok(()) => Ok(()),
            Err(TrySendError::Full(_)) => Err(SubmitError::Backpressure),
            Err(TrySendError::Disconnected(_)) => Err(SubmitError::Stopped),
        }
    }
}

impl Drop for EngineHost {
    fn drop(&mut self) {
        let _ = self.sender.send(EngineCommand::Shutdown);
        if let Some(thread) = self.thread.lock().unwrap().take() {
            let _ = thread.join();
        }
    }
}

fn run_engine<R: RendererBackend>(
    receiver: Receiver<EngineCommand>,
    mut renderer: R,
    stats: Arc<Mutex<EngineStats>>,
) {
    while let Ok(command) = receiver.recv() {
        let result = match command {
            EngineCommand::AttachSurface(surface) => renderer.attach_surface(surface),
            EngineCommand::DetachSurface => {
                renderer.detach_surface();
                Ok(())
            }
            EngineCommand::Resize { width, height } => renderer.resize(width, height),
            EngineCommand::Input(samples) => {
                {
                    let mut stats = stats.lock().unwrap();
                    stats.input_packets += 1;
                    stats.input_samples += samples.len() as u64;
                }
                renderer.ingest_input(&samples)
            }
            EngineCommand::Shutdown => {
                renderer.detach_surface();
                break;
            }
        };
        if result.is_err() {
            stats.lock().unwrap().renderer_errors += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use inkframe_core::sample_flags;

    #[test]
    fn rejects_zero_sized_surfaces() {
        let engine = EngineHost::spawn(NullRenderer);
        assert_eq!(
            engine.attach_surface(NativeSurface { handle: 1, width: 0, height: 10 }),
            Err(SubmitError::InvalidSurfaceSize)
        );
    }

    #[test]
    fn accepts_input_without_blocking_caller() {
        let engine = EngineHost::spawn(NullRenderer);
        let sample = StrokeSample {
            x: 1.0,
            y: 2.0,
            pressure: 1.0,
            tilt: 0.0,
            orientation: 0.0,
            time_ns: 0,
            flags: sample_flags::MOVE,
        };
        assert!(engine.submit_input(vec![sample]).is_ok());
    }
}
