//! The recognition process. transcribe.cpp can't be unloaded from a process
//! (its backend modules and the Vulkan driver stay for good, about 40 MB),
//! so it runs in a second Typr process (`typr --local-recognition`) that
//! holds one model and exits when the model is unloaded: an idle Typr is
//! back to its own few megabytes. A crash in a GPU driver ends only that
//! process.
//!
//! The two talk over the child's stdin/stdout in frames: a little-endian
//! u32 length, then the bytes. Messages are JSON; the samples of a
//! transcription follow their request as one frame of little-endian f32.
//! The child's log records travel as messages too and land in Typr's log.

use serde::{Deserialize, Serialize};
use std::io::{self, BufRead, BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, ExitStatus, Stdio};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use super::catalog::Backend;
use super::ffi::{self, Api, Device, Loaded};
use super::{lock, megabytes, runtime};

/// First argument that starts Typr as the recognition process
pub const ARG: &str = "--local-recognition";

/// No message comes near this; a bigger length means a broken stream
const MAX_FRAME: usize = 512 << 20;
/// How long a process may take to free its model and exit
const EXIT_TIMEOUT: Duration = Duration::from_secs(3);

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
enum Request {
    /// The first message: load the runtime and the model
    Load {
        runtime_dir: PathBuf,
        model_path: PathBuf,
        backend: Backend,
    },
    /// Followed by a frame with the samples
    Transcribe,
}

/// Where the model runs.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Placement {
    /// The backend it actually runs on, e.g. "Vulkan" or "CPU"
    pub backend: String,
    pub device: String,
    /// Why the requested GPU backend isn't used, when it fell back to the CPU
    pub fallback: Option<String>,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
enum Reply {
    Log {
        level: String,
        target: String,
        message: String,
    },
    Loaded {
        placement: Placement,
    },
    Text {
        text: String,
        placement: Placement,
    },
    Error {
        message: String,
    },
}

fn write_frame(out: &mut impl Write, bytes: &[u8]) -> io::Result<()> {
    let length = u32::try_from(bytes.len())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "frame too large"))?;
    out.write_all(&length.to_le_bytes())?;
    out.write_all(bytes)?;
    out.flush()
}

/// The next frame, or `None` when the stream ended between frames.
fn read_frame(input: &mut impl Read) -> io::Result<Option<Vec<u8>>> {
    let mut length = [0u8; 4];
    match input.read_exact(&mut length) {
        Ok(()) => {}
        Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(e) => return Err(e),
    }
    let length = u32::from_le_bytes(length) as usize;
    if length > MAX_FRAME {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("a frame of {} bytes", length),
        ));
    }
    let mut bytes = vec![0u8; length];
    input.read_exact(&mut bytes)?;
    Ok(Some(bytes))
}

fn samples_to_bytes(samples: &[f32]) -> Vec<u8> {
    samples.iter().flat_map(|s| s.to_le_bytes()).collect()
}

fn bytes_to_samples(bytes: &[u8]) -> Result<Vec<f32>, String> {
    if bytes.len() % 4 != 0 {
        return Err(format!("{} bytes of samples aren't whole f32 values", bytes.len()));
    }
    Ok(bytes
        .chunks_exact(4)
        .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        .collect())
}

/// "vulkan" → "Vulkan", "cpu" → "CPU"
fn backend_label(name: &str) -> String {
    Backend::parse(&name.to_ascii_lowercase())
        .map(|b| b.label().to_string())
        .unwrap_or_else(|| name.to_string())
}

fn describe_exit(status: ExitStatus) -> String {
    match status.code() {
        // Windows crash codes (NTSTATUS) read best in hex
        Some(code) if !(0..=255).contains(&code) => format!("exit code {:#x}", code as u32),
        Some(code) => format!("exit code {}", code),
        None => "no exit code".to_string(),
    }
}

// ── Typr's side ──────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum WorkerError {
    /// The process answered with an error (a missing file, a bad model)
    Failed(String),
    /// The process ended or stopped answering
    Crashed(String),
}

impl WorkerError {
    pub fn message(&self) -> &str {
        match self {
            WorkerError::Failed(message) | WorkerError::Crashed(message) => message,
        }
    }
}

/// A running recognition process with one model loaded. Dropping it closes
/// the process's input: it frees the model and exits.
pub struct Worker {
    child: Child,
    input: Option<BufWriter<ChildStdin>>,
    output: BufReader<ChildStdout>,
    placement: Placement,
}

impl Worker {
    /// Starts `exe` as the recognition process and loads the model in it.
    pub fn start(exe: &Path, runtime_dir: &Path, model_path: &Path, backend: Backend) -> Result<Worker, WorkerError> {
        let mut command = Command::new(exe);
        command
            .arg(ARG)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            command.creation_flags(CREATE_NO_WINDOW);
        }
        let mut child = command.spawn().map_err(|e| {
            WorkerError::Failed(format!("Could not start the local recognition process: {}", e))
        })?;
        log::debug!("Local recognition process {} started", child.id());

        // Whatever the libraries print themselves (e.g. before aborting)
        if let Some(stderr) = child.stderr.take() {
            std::thread::spawn(move || {
                for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                    if !line.trim().is_empty() {
                        log::warn!(target: "typr_lib::transcribe.cpp", "{}", line.trim_end());
                    }
                }
            });
        }
        let input = child.stdin.take().expect("stdin is piped");
        let output = child.stdout.take().expect("stdout is piped");
        let mut worker = Worker {
            child,
            input: Some(BufWriter::new(input)),
            output: BufReader::new(output),
            placement: Placement::default(),
        };

        worker.send(&Request::Load {
            runtime_dir: runtime_dir.to_path_buf(),
            model_path: model_path.to_path_buf(),
            backend,
        })?;
        match worker.reply()? {
            Reply::Loaded { placement } => {
                worker.placement = placement;
                Ok(worker)
            }
            Reply::Error { message } => Err(WorkerError::Failed(message)),
            other => Err(WorkerError::Crashed(format!("unexpected answer {:?}", other))),
        }
    }

    pub fn placement(&self) -> &Placement {
        &self.placement
    }

    /// Marks a CPU process that stands in for a GPU one
    pub fn set_fallback(&mut self, reason: String) {
        self.placement.fallback = Some(reason);
    }

    /// Transcribes 16 kHz mono samples. A GPU failure inside the process
    /// moves the model to the CPU there; `placement` then says so.
    pub fn transcribe(&mut self, samples: &[f32]) -> Result<String, WorkerError> {
        self.send(&Request::Transcribe)?;
        self.send_bytes(&samples_to_bytes(samples))?;
        match self.reply()? {
            Reply::Text { text, placement } => {
                self.placement = placement;
                Ok(text)
            }
            Reply::Error { message } => Err(WorkerError::Failed(message)),
            other => Err(WorkerError::Crashed(format!("unexpected answer {:?}", other))),
        }
    }

    fn send(&mut self, request: &Request) -> Result<(), WorkerError> {
        let bytes = serde_json::to_vec(request).expect("requests serialize");
        self.send_bytes(&bytes)
    }

    fn send_bytes(&mut self, bytes: &[u8]) -> Result<(), WorkerError> {
        let input = self.input.as_mut().expect("the input is open while the worker lives");
        if let Err(e) = write_frame(input, bytes) {
            return Err(self.crashed(&e.to_string()));
        }
        Ok(())
    }

    /// The next answer; log records on the way go to Typr's log.
    fn reply(&mut self) -> Result<Reply, WorkerError> {
        loop {
            let frame = match read_frame(&mut self.output) {
                Ok(Some(frame)) => frame,
                Ok(None) => return Err(self.crashed("the output ended")),
                Err(e) => return Err(self.crashed(&e.to_string())),
            };
            let reply: Reply = serde_json::from_slice(&frame)
                .map_err(|e| self.crashed(&format!("unreadable answer: {}", e)))?;
            match reply {
                Reply::Log { level, target, message } => {
                    let level = level.parse().unwrap_or(log::Level::Info);
                    log::log!(target: &target, level, "{}", message);
                }
                other => return Ok(other),
            }
        }
    }

    fn crashed(&mut self, what: &str) -> WorkerError {
        // The exit code says more than a broken pipe, when there is one
        let deadline = Instant::now() + Duration::from_millis(500);
        while Instant::now() < deadline {
            if let Ok(Some(status)) = self.child.try_wait() {
                return WorkerError::Crashed(format!(
                    "the local recognition process stopped ({})",
                    describe_exit(status)
                ));
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        WorkerError::Crashed(format!("the local recognition process stopped answering ({})", what))
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        // End of input: the process frees the model and exits
        drop(self.input.take());
        let deadline = Instant::now() + EXIT_TIMEOUT;
        loop {
            match self.child.try_wait() {
                Ok(Some(status)) => {
                    log::debug!("Local recognition process {} ended ({})", self.child.id(), describe_exit(status));
                    return;
                }
                Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(20)),
                _ => {
                    log::warn!("Local recognition process {} didn't exit, stopping it", self.child.id());
                    let _ = self.child.kill();
                    let _ = self.child.wait();
                    return;
                }
            }
        }
    }
}

// ── The recognition process ──────────────────────────

type Output = Arc<Mutex<BufWriter<io::Stdout>>>;

fn send(output: &Output, reply: &Reply) {
    let bytes = serde_json::to_vec(reply).expect("replies serialize");
    // Typr is gone when this fails; the next read ends the process
    let _ = write_frame(&mut *lock(output), &bytes);
}

/// Sends log records to Typr, which logs them as its own.
struct ForwardLogger {
    output: Output,
}

impl log::Log for ForwardLogger {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        metadata.level() <= log::Level::Debug
    }

    fn log(&self, record: &log::Record) {
        if !self.enabled(record.metadata()) {
            return;
        }
        send(
            &self.output,
            &Reply::Log {
                level: record.level().as_str().to_string(),
                target: record.target().to_string(),
                message: record.args().to_string(),
            },
        );
    }

    fn flush(&self) {}
}

/// The model loaded in the recognition process.
struct Session {
    api: &'static Api,
    devices: Vec<Device>,
    model_path: PathBuf,
    loaded: Option<Loaded>,
    placement: Placement,
}

impl Session {
    fn open(runtime_dir: &Path, model_path: &Path, backend: Backend) -> Result<Session, String> {
        runtime::read_contract(runtime_dir)?;
        let started = Instant::now();
        let api = Api::open(runtime_dir)?;
        let devices = api.devices();
        log::info!(
            "transcribe.cpp runtime loaded in {} ms, devices: {}",
            started.elapsed().as_millis(),
            devices
                .iter()
                .map(|d| format!("{} [{}] {} ({} MB)", d.name, d.kind, d.description.trim(), megabytes(d.memory_total)))
                .collect::<Vec<_>>()
                .join("; ")
        );
        let mut session = Session {
            api,
            devices,
            model_path: model_path.to_path_buf(),
            loaded: None,
            placement: Placement::default(),
        };
        session.load(backend)?;
        Ok(session)
    }

    /// The device for `backend`: a discrete GPU before an integrated one
    fn device(&self, backend: Backend) -> Option<&Device> {
        self.devices
            .iter()
            .filter(|d| d.kind == backend.id())
            .min_by_key(|d| match d.device_type {
                ffi::DEVICE_TYPE_GPU => 0,
                ffi::DEVICE_TYPE_IGPU => 1,
                ffi::DEVICE_TYPE_CPU => 2,
                _ => 3,
            })
    }

    /// Loads the model on `backend`, or on the CPU when the GPU can't take
    /// it, so a dictation still works.
    fn load(&mut self, backend: Backend) -> Result<(), String> {
        let started = Instant::now();
        let path = self.model_path.clone();
        let cpu = |session: &Session| {
            session
                .api
                .load_model(&path, Backend::Cpu, session.device(Backend::Cpu))
                .map_err(|e| e.message)
        };
        let (loaded, fallback) = if backend == Backend::Cpu {
            (cpu(self)?, None)
        } else {
            let attempt = match self.device(backend) {
                None => Err(format!("{} found no device", backend.label())),
                Some(device) => self.api.load_model(&path, backend, Some(device)).map_err(|e| e.message),
            };
            match attempt {
                Ok(loaded) => (loaded, None),
                Err(reason) => {
                    log::warn!("{}; loading the model on the CPU instead", reason);
                    (cpu(self)?, Some(reason))
                }
            }
        };

        // The device's kind ("vulkan"), not its name ("Vulkan0")
        let placed = self.api.device_of(&loaded);
        let actual = backend_label(
            &placed
                .as_ref()
                .map(|d| d.kind.clone())
                .unwrap_or_else(|| self.api.backend_of(&loaded)),
        );
        let device = placed
            .map(|d| if d.description.trim().is_empty() { d.name } else { d.description })
            .unwrap_or_default()
            .trim()
            .to_string();
        log::info!("Model loaded on {} ({}) in {} ms", actual, device, started.elapsed().as_millis());
        self.placement = Placement {
            backend: actual,
            device,
            fallback,
        };
        self.loaded = Some(loaded);
        Ok(())
    }

    fn free(&mut self) {
        if let Some(loaded) = self.loaded.take() {
            self.api.free(loaded);
        }
    }

    fn transcribe(&mut self, samples: &[f32]) -> Result<String, String> {
        let started = Instant::now();
        let loaded = self.loaded.as_mut().ok_or("No model is loaded")?;
        let transcript = match self.api.transcribe(loaded, samples) {
            Err(e) if e.is_backend_failure() && self.placement.backend != Backend::Cpu.label() => {
                // The library's advice: reload on the CPU and retry
                let gpu = self.placement.backend.clone();
                log::warn!("{} failed during transcription ({}), retrying on the CPU", gpu, e);
                self.free();
                self.load(Backend::Cpu)?;
                self.placement.fallback = Some(format!("{} failed during transcription: {}", gpu, e));
                let loaded = self.loaded.as_mut().ok_or("No model is loaded")?;
                self.api.transcribe(loaded, samples)
            }
            other => other,
        }
        .map_err(|e| e.message)?;

        log::info!(
            "Local transcription took {} ms for {:.1} s of audio{}",
            started.elapsed().as_millis(),
            samples.len() as f32 / crate::audio::TARGET_SAMPLE_RATE as f32,
            if transcript.language.is_empty() {
                String::new()
            } else {
                format!(" (language: {})", transcript.language)
            }
        );
        if let Some(warning) = transcript.warning {
            log::warn!("The transcript may be incomplete: {}", warning);
        }
        Ok(transcript.text)
    }
}

fn read_request(input: &mut impl Read) -> Result<Option<Request>, String> {
    match read_frame(input).map_err(|e| e.to_string())? {
        None => Ok(None),
        Some(frame) => serde_json::from_slice(&frame)
            .map(Some)
            .map_err(|e| format!("unreadable request: {}", e)),
    }
}

/// Entry point of the recognition process: loads the model the first
/// request names, then transcribes until Typr closes the input. Returns the
/// exit code.
pub fn run() -> i32 {
    let output: Output = Arc::new(Mutex::new(BufWriter::new(io::stdout())));
    static LOGGER: OnceLock<ForwardLogger> = OnceLock::new();
    let logger = LOGGER.get_or_init(|| ForwardLogger { output: output.clone() });
    if log::set_logger(logger).is_ok() {
        log::set_max_level(log::LevelFilter::Debug);
    }
    let mut input = BufReader::new(io::stdin());

    let (runtime_dir, model_path, backend) = match read_request(&mut input) {
        Ok(Some(Request::Load {
            runtime_dir,
            model_path,
            backend,
        })) => (runtime_dir, model_path, backend),
        Ok(None) => return 0,
        Ok(Some(other)) => {
            send(&output, &Reply::Error { message: format!("expected Load first, got {:?}", other) });
            return 2;
        }
        Err(e) => {
            send(&output, &Reply::Error { message: e });
            return 2;
        }
    };
    let mut session = match Session::open(&runtime_dir, &model_path, backend) {
        Ok(session) => session,
        Err(message) => {
            send(&output, &Reply::Error { message });
            return 1;
        }
    };
    send(&output, &Reply::Loaded { placement: session.placement.clone() });

    loop {
        let reply = match read_request(&mut input) {
            Ok(None) => break,
            Ok(Some(Request::Transcribe)) => {
                let samples = read_frame(&mut input)
                    .map_err(|e| e.to_string())
                    .and_then(|frame| frame.ok_or_else(|| "the samples are missing".to_string()))
                    .and_then(|bytes| bytes_to_samples(&bytes));
                match samples.and_then(|samples| session.transcribe(&samples)) {
                    Ok(text) => Reply::Text {
                        text,
                        placement: session.placement.clone(),
                    },
                    Err(message) => Reply::Error { message },
                }
            }
            Ok(Some(Request::Load { .. })) => Reply::Error {
                message: "A model is loaded already".to_string(),
            },
            Err(e) => {
                log::error!("Stopping: {}", e);
                break;
            }
        };
        send(&output, &reply);
    }
    session.free();
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_frames_round_trip() {
        let mut stream = Vec::new();
        write_frame(&mut stream, b"hello").unwrap();
        write_frame(&mut stream, b"").unwrap();
        let mut reader = &stream[..];
        assert_eq!(read_frame(&mut reader).unwrap(), Some(b"hello".to_vec()));
        assert_eq!(read_frame(&mut reader).unwrap(), Some(Vec::new()));
        assert_eq!(read_frame(&mut reader).unwrap(), None);
    }

    #[test]
    fn test_cut_frame_is_an_error() {
        let mut stream = Vec::new();
        write_frame(&mut stream, b"hello").unwrap();
        stream.truncate(6);
        assert!(read_frame(&mut &stream[..]).is_err());
        let huge = (u32::MAX).to_le_bytes();
        assert!(read_frame(&mut &huge[..]).is_err());
    }

    #[test]
    fn test_samples_round_trip() {
        let samples = [0.0f32, -1.0, 0.5, f32::MIN_POSITIVE];
        assert_eq!(bytes_to_samples(&samples_to_bytes(&samples)).unwrap(), samples);
        assert!(bytes_to_samples(&[0, 0, 0]).is_err());
    }

    #[test]
    fn test_messages_round_trip() {
        let request = Request::Load {
            runtime_dir: PathBuf::from(r"C:\runtime"),
            model_path: PathBuf::from(r"C:\models\model.gguf"),
            backend: Backend::Vulkan,
        };
        let json = serde_json::to_vec(&request).unwrap();
        assert_eq!(serde_json::from_slice::<Request>(&json).unwrap(), request);

        let reply = Reply::Text {
            text: "Hello".to_string(),
            placement: Placement {
                backend: "CPU".to_string(),
                device: "Ryzen".to_string(),
                fallback: Some("Vulkan found no device".to_string()),
            },
        };
        let json = serde_json::to_vec(&reply).unwrap();
        assert_eq!(serde_json::from_slice::<Reply>(&json).unwrap(), reply);
    }

    #[test]
    fn test_backend_label() {
        assert_eq!(backend_label("vulkan"), "Vulkan");
        assert_eq!(backend_label("cpu"), "CPU");
        assert_eq!(backend_label("metal"), "metal");
    }
}
