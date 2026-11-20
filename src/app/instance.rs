//! Per-user instance election and bounded local IPC. The OS lock, not the socket
//! file or a PID, decides ownership, including simultaneous launches and crashes.

use std::{
    fs::{self, File, OpenOptions},
    io::{self, Read, Seek, SeekFrom, Write},
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

use interprocess::local_socket::{
    GenericFilePath, Listener, ListenerNonblockingMode, ListenerOptions, Stream, prelude::*,
};

use super::windows::{OpenMode, OpenRequest, OpenTarget};

const TIMEOUT: Duration = Duration::from_secs(3);
const POLL: Duration = Duration::from_millis(50);
const MAX_PATH: usize = 64 * 1024;
const MAGIC: &[u8; 5] = b"KVER\x01";

pub(super) enum Instance {
    Primary(Box<Primary>),
    Secondary(Stream),
}

pub(super) struct Primary {
    listener: Listener,
    // Drop the socket before its directory, and release ownership last.
    _directory: tempfile::TempDir,
    _lock: File,
}

pub(super) struct Server {
    stop: Arc<AtomicBool>,
    worker: Option<thread::JoinHandle<()>>,
}

impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

pub(super) fn connect_or_listen(data: &Path) -> io::Result<Instance> {
    fs::create_dir_all(data)?;
    let mut options = OpenOptions::new();
    options.read(true).write(true).create(true).truncate(false);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.mode(0o600);
    }
    let mut lock = options.open(data.join("instance.lock"))?;
    let deadline = Instant::now() + TIMEOUT;
    loop {
        match lock.try_lock() {
            Ok(()) => {
                // A private, short path avoids Unix socket path limits even when
                // the user's application-data path is long. A crashed process's
                // stale endpoint is ignored because the lock is authoritative.
                #[cfg(unix)]
                let directory = tempfile::Builder::new()
                    .prefix("kver-")
                    .tempdir_in("/tmp")?;
                #[cfg(windows)]
                let directory = tempfile::Builder::new().prefix("kver-").tempdir()?;
                #[cfg(unix)]
                let endpoint = directory.path().join("cli.sock");
                #[cfg(windows)]
                let endpoint = std::path::PathBuf::from(format!(
                    r"\\.\pipe\{}",
                    directory.path().file_name().unwrap().to_string_lossy()
                ));
                let listener = ListenerOptions::new()
                    .name(endpoint.as_os_str().to_fs_name::<GenericFilePath>()?)
                    .nonblocking(ListenerNonblockingMode::Accept)
                    .create_sync()?;
                lock.set_len(0)?;
                lock.seek(SeekFrom::Start(0))?;
                lock.write_all(endpoint.to_string_lossy().as_bytes())?;
                lock.flush()?;
                return Ok(Instance::Primary(Box::new(Primary {
                    listener,
                    _directory: directory,
                    _lock: lock,
                })));
            }
            Err(std::fs::TryLockError::WouldBlock) => {}
            Err(std::fs::TryLockError::Error(error)) => return Err(error),
        }
        lock.seek(SeekFrom::Start(0))?;
        let mut endpoint = String::new();
        (&mut lock).take(4096).read_to_string(&mut endpoint)?;
        if !endpoint.is_empty()
            && let Ok(stream) = connect(&endpoint)
        {
            return Ok(Instance::Secondary(stream));
        }
        if Instant::now() >= deadline {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "another Kver process owns the instance lock but is not accepting requests",
            ));
        }
        thread::sleep(POLL);
    }
}

#[cfg(unix)]
fn connect(endpoint: &str) -> io::Result<Stream> {
    Stream::connect(endpoint.to_fs_name::<GenericFilePath>()?)
}

#[cfg(windows)]
fn connect(endpoint: &str) -> io::Result<Stream> {
    use interprocess::{
        ConnectWaitMode,
        os::windows::named_pipe::{DuplexPipeStream, local_socket, pipe_mode::Bytes},
    };
    use std::os::windows::io::OwnedHandle;
    // The generic Stream::connect waits indefinitely for a busy named pipe.
    // Let the election loop own retries and its total connection deadline.
    let pipe = DuplexPipeStream::<Bytes>::connect_by_path_with_wait_mode(
        endpoint,
        ConnectWaitMode::Timeout(Duration::ZERO),
    )?;
    let handle = OwnedHandle::try_from(pipe)
        .map_err(|_| io::Error::other("cannot take ownership of CLI pipe"))?;
    let stream = local_socket::Stream::try_from(handle)?;
    Ok(Stream::NamedPipe(stream))
}

/// Named pipes do not support socket timeouts. Nonblocking I/O with one
/// deadline also prevents a trickling client from extending each read forever.
struct DeadlineStream {
    stream: Stream,
    deadline: Instant,
}

impl DeadlineStream {
    fn new(stream: Stream) -> io::Result<Self> {
        stream.set_nonblocking(true)?;
        Ok(Self {
            stream,
            deadline: Instant::now() + TIMEOUT,
        })
    }
}

fn until_ready<T>(
    deadline: Instant,
    mut operation: impl FnMut() -> io::Result<T>,
) -> io::Result<T> {
    loop {
        if Instant::now() >= deadline {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "CLI request timed out",
            ));
        }
        match operation() {
            Err(error) if would_block(&error) => {
                thread::sleep(Duration::from_millis(5));
            }
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            result => return result,
        }
    }
}

fn would_block(error: &io::Error) -> bool {
    // PIPE_NOWAIT reports an empty pipe as ERROR_NO_DATA, which std does not
    // classify as WouldBlock. A disconnected empty pipe also times out safely.
    #[cfg(windows)]
    if error.raw_os_error() == Some(232) {
        return true;
    }
    error.kind() == io::ErrorKind::WouldBlock
}

impl Read for DeadlineStream {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        until_ready(self.deadline, || self.stream.read(bytes))
    }
}

impl Write for DeadlineStream {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        until_ready(self.deadline, || {
            let written = self.stream.write(bytes)?;
            // A nonblocking byte-mode named pipe can successfully write zero
            // bytes when its buffer is full; retry instead of reporting WriteZero.
            if written == 0 && !bytes.is_empty() {
                return Err(io::ErrorKind::WouldBlock.into());
            }
            Ok(written)
        })
    }

    fn flush(&mut self) -> io::Result<()> {
        // LocalSocketStream is unbuffered; acceptance is confirmed by the ACK.
        Ok(())
    }
}

impl Primary {
    pub(super) fn start(self, requests: async_channel::Sender<OpenRequest>) -> io::Result<Server> {
        let stop = Arc::new(AtomicBool::new(false));
        let stopping = stop.clone();
        let worker = thread::Builder::new()
            .name("kver-cli".into())
            .spawn(move || {
                while !stopping.load(Ordering::Relaxed) {
                    match self.listener.accept() {
                        Ok(stream) => {
                            let result = (|| {
                                let mut stream = DeadlineStream::new(stream)?;
                                let request = read_request(&mut stream)?;
                                requests.try_send(request).map_err(io::Error::other)?;
                                stream.write_all(b"OK")
                            })();
                            if let Err(error) = result {
                                eprintln!("Cannot accept CLI request: {error}");
                            }
                        }
                        Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                            thread::sleep(POLL)
                        }
                        Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                        Err(error) => {
                            eprintln!("Cannot listen for CLI requests: {error}");
                            break;
                        }
                    }
                }
                // Keep the election lock and endpoint alive for the worker's lifetime.
                drop(self);
            })?;
        Ok(Server {
            stop,
            worker: Some(worker),
        })
    }
}

/// Success means queued by the app, not that PDF loading or a Save/Discard
/// decision has completed. Do not retry after sending: that could open twice.
pub(super) fn forward(stream: Stream, request: &OpenRequest) -> io::Result<()> {
    let mut stream = DeadlineStream::new(stream)?;
    write_request(&mut stream, request)?;
    let mut ack = [0; 2];
    stream.read_exact(&mut ack)?;
    if &ack != b"OK" {
        return Err(invalid("Kver returned an invalid acknowledgement"));
    }
    Ok(())
}

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

fn write_request(writer: &mut impl Write, request: &OpenRequest) -> io::Result<()> {
    // Use a distinct magic for URL requests. Local requests remain compatible
    // with older running instances; they cannot mistake a URL for a filename.
    let (magic, path) = match &request.target {
        Some(OpenTarget::File(path)) => (MAGIC, encode_path(path)),
        Some(OpenTarget::Url(url)) => (b"KURL\x01", url.as_str().as_bytes().to_vec()),
        None => (MAGIC, Vec::new()),
    };
    if path.len() > MAX_PATH {
        return Err(invalid("PDF path is too long"));
    }
    writer.write_all(magic)?;
    writer.write_all(&[match request.mode {
        OpenMode::Default => 0,
        OpenMode::New => 1,
        OpenMode::Reuse => 2,
    }])?;
    writer.write_all(&(path.len() as u32).to_le_bytes())?;
    writer.write_all(&path)
}

fn read_request(reader: &mut impl Read) -> io::Result<OpenRequest> {
    let mut header = [0; 10];
    reader.read_exact(&mut header)?;
    let remote = &header[..5] == b"KURL\x01";
    if !remote && &header[..5] != MAGIC {
        return Err(invalid("unsupported CLI protocol version"));
    }
    let mode = match header[5] {
        0 => OpenMode::Default,
        1 => OpenMode::New,
        2 => OpenMode::Reuse,
        _ => return Err(invalid("invalid window mode")),
    };
    let length = u32::from_le_bytes(header[6..10].try_into().unwrap()) as usize;
    if length > MAX_PATH {
        return Err(invalid("PDF path is too long"));
    }
    let target = if remote {
        let mut bytes = vec![0; length];
        reader.read_exact(&mut bytes)?;
        let value = std::str::from_utf8(&bytes).map_err(|_| invalid("invalid URL encoding"))?;
        Some(OpenTarget::Url(
            super::remote_pdf::parse_url(value).map_err(|_| invalid("invalid PDF URL"))?,
        ))
    } else if length == 0 {
        None
    } else {
        let mut bytes = vec![0; length];
        reader.read_exact(&mut bytes)?;
        let path = decode_path(bytes)?;
        if !path.is_absolute() {
            return Err(invalid("CLI paths must be absolute"));
        }
        Some(OpenTarget::File(path))
    };
    Ok(OpenRequest { target, mode })
}

#[cfg(unix)]
fn encode_path(path: &Path) -> Vec<u8> {
    use std::os::unix::ffi::OsStrExt as _;
    path.as_os_str().as_bytes().to_vec()
}

#[cfg(unix)]
fn decode_path(bytes: Vec<u8>) -> io::Result<std::path::PathBuf> {
    use std::os::unix::ffi::OsStringExt as _;
    Ok(std::ffi::OsString::from_vec(bytes).into())
}

#[cfg(windows)]
fn encode_path(path: &Path) -> Vec<u8> {
    use std::os::windows::ffi::OsStrExt as _;
    path.as_os_str()
        .encode_wide()
        .flat_map(u16::to_le_bytes)
        .collect()
}

#[cfg(windows)]
fn decode_path(bytes: Vec<u8>) -> io::Result<std::path::PathBuf> {
    use std::os::windows::ffi::OsStringExt as _;
    if !bytes.len().is_multiple_of(2) {
        return Err(invalid("invalid Windows path encoding"));
    }
    let wide: Vec<_> = bytes
        .chunks_exact(2)
        .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
        .collect();
    Ok(std::ffi::OsString::from_wide(&wide).into())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn primary(data: &Path) -> Primary {
        match connect_or_listen(data).unwrap() {
            Instance::Primary(primary) => *primary,
            Instance::Secondary(_) => panic!("expected primary"),
        }
    }

    #[test]
    fn simultaneous_processes_elect_one_owner_and_forward_every_request() {
        const CHILD_DATA: &str = "KVER_INSTANCE_TEST_DATA";
        if let Some(data) = std::env::var_os(CHILD_DATA) {
            let data = std::path::PathBuf::from(data);
            match connect_or_listen(&data).unwrap() {
                Instance::Primary(primary) => {
                    // A second owner, even after the first has exited, fails.
                    File::create_new(data.join("elected")).unwrap();
                    let (tx, rx) = async_channel::unbounded();
                    let _server = primary.start(tx).unwrap();
                    let deadline = Instant::now() + Duration::from_secs(15);
                    let mut received = 0;
                    while received < 3 {
                        if let Ok(request) = rx.try_recv() {
                            assert_eq!(request.mode, OpenMode::Reuse);
                            received += 1;
                        } else {
                            assert!(Instant::now() < deadline, "missing forwarded requests");
                            thread::sleep(POLL);
                        }
                    }
                }
                Instance::Secondary(stream) => {
                    forward(
                        stream,
                        &OpenRequest {
                            target: Some(OpenTarget::File(
                                std::path::absolute("space # % æ.pdf").unwrap(),
                            )),
                            mode: OpenMode::Reuse,
                        },
                    )
                    .unwrap();
                }
            }
            return;
        }
        let data = tempfile::tempdir().unwrap();
        let children: Vec<_> = (0..4).map(|_| {
            std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "app::instance::tests::simultaneous_processes_elect_one_owner_and_forward_every_request", "--nocapture"])
                .env(CHILD_DATA, data.path())
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn().unwrap()
        }).collect();
        for child in children {
            let output = child.wait_with_output().unwrap();
            assert!(
                output.status.success(),
                "{}\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        }
        assert!(data.path().join("elected").exists());
        drop(primary(data.path()));
    }

    #[test]
    fn unresponsive_peer_has_a_total_deadline() {
        let start = Instant::now();
        let result: io::Result<()> = until_ready(start + Duration::from_millis(15), || {
            Err(io::ErrorKind::WouldBlock.into())
        });
        assert_eq!(result.unwrap_err().kind(), io::ErrorKind::TimedOut);
    }

    #[test]
    fn handoff_acknowledges_queued_request_and_owner_can_restart() {
        let data = tempfile::tempdir().unwrap();
        let (tx, rx) = async_channel::unbounded();
        let server = primary(data.path()).start(tx).unwrap();
        let request = OpenRequest {
            target: Some(OpenTarget::File(
                std::path::absolute("space # % æ.pdf").unwrap(),
            )),
            mode: OpenMode::Reuse,
        };
        for _ in 0..2 {
            let Instance::Secondary(stream) = connect_or_listen(data.path()).unwrap() else {
                panic!("second invocation must not become primary");
            };
            forward(stream, &request).unwrap();
            assert_eq!(rx.recv_blocking().unwrap(), request);
        }
        drop(server);
        // The lock file is deliberately retained; its stale contents cannot
        // prevent a new owner from launching.
        drop(primary(data.path()));
    }

    #[test]
    fn protocol_rejects_malformed_oversized_relative_and_truncated_requests() {
        for request in [
            b"wrong header".to_vec(),
            [MAGIC.as_slice(), &[9], &0_u32.to_le_bytes()].concat(),
            [MAGIC.as_slice(), &[0], &u32::MAX.to_le_bytes()].concat(),
            [MAGIC.as_slice(), &[0], &1_u32.to_le_bytes()].concat(),
        ] {
            assert!(read_request(&mut request.as_slice()).is_err());
        }
        let mut bytes = Vec::new();
        write_request(
            &mut bytes,
            &OpenRequest {
                target: Some(OpenTarget::File("relative.pdf".into())),
                mode: OpenMode::Default,
            },
        )
        .unwrap();
        assert!(read_request(&mut bytes.as_slice()).is_err());
    }

    #[test]
    fn protocol_round_trips_empty_and_native_paths() {
        let mut paths = vec![None, Some(std::path::absolute("space # % æ.pdf").unwrap())];
        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStringExt as _;
            paths.push(Some(
                std::path::absolute(std::ffi::OsString::from_vec(b"non-utf8-\xff.pdf".to_vec()))
                    .unwrap(),
            ));
        }
        let targets = paths
            .into_iter()
            .map(|path| path.map(OpenTarget::File))
            .chain([Some(OpenTarget::Url(
                url::Url::parse("https://example.org/pdf?q=1").unwrap(),
            ))]);
        for target in targets {
            for mode in [OpenMode::Default, OpenMode::New, OpenMode::Reuse] {
                let request = OpenRequest {
                    target: target.clone(),
                    mode,
                };
                let mut bytes = Vec::new();
                write_request(&mut bytes, &request).unwrap();
                assert_eq!(read_request(&mut bytes.as_slice()).unwrap(), request);
            }
        }
    }
}
