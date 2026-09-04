use std::collections::{HashMap, HashSet};
use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::{self, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender, SyncSender};
use std::thread::{self, JoinHandle};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use sha2::{Digest, Sha256};
use tracing::{Event, Level, Subscriber};
use tracing_subscriber::filter::EnvFilter;
use tracing_subscriber::layer::{Context, Layer};
use tracing_subscriber::prelude::*;

const LOG_DIRECTORY_NAME: &str = "aio-asset-normalizer";
const LOG_SUBDIRECTORY_NAME: &str = "logs";
const MAX_LOG_FILE_BYTES: u64 = 10 * 1024 * 1024;
const MAX_LOG_BACKUPS: u32 = 3;
const UI_QUEUE_CAPACITY: usize = 8_192;

static NEXT_TASK_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum LogTarget {
    App,
    GlbEditor,
    GlbExport,
    BvhStudio,
    Retarget,
    GlbRetarget,
    RetargetAgent,
    FbxConverter,
}

impl LogTarget {
    pub(crate) fn parse(value: &str) -> Self {
        match value {
            "glb_editor" => Self::GlbEditor,
            "glb_export" => Self::GlbExport,
            "bvh_studio" => Self::BvhStudio,
            "retarget" => Self::Retarget,
            "glb_retarget" => Self::GlbRetarget,
            "retarget_agent" => Self::RetargetAgent,
            "fbx_converter" => Self::FbxConverter,
            _ => Self::App,
        }
    }

    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::App => "app",
            Self::GlbEditor => "glb_editor",
            Self::GlbExport => "glb_export",
            Self::BvhStudio => "bvh_studio",
            Self::Retarget => "retarget",
            Self::GlbRetarget => "glb_retarget",
            Self::RetargetAgent => "retarget_agent",
            Self::FbxConverter => "fbx_converter",
        }
    }

    fn file_name(self) -> Option<&'static str> {
        match self {
            Self::App => None,
            Self::GlbEditor => Some("glb-editor.log"),
            Self::GlbExport => Some("glb-export.log"),
            Self::BvhStudio => Some("bvh-studio.log"),
            Self::Retarget | Self::GlbRetarget | Self::RetargetAgent => {
                Some("retarget.log")
            }
            Self::FbxConverter => Some("fbx-converter.log"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum LogLevel {
    Debug,
    Info,
    Warn,
    Error,
}

impl LogLevel {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Debug => "DEBUG",
            Self::Info => "INFO",
            Self::Warn => "WARN",
            Self::Error => "ERROR",
        }
    }

    pub(crate) fn from_tracing(level: &Level) -> Self {
        match *level {
            Level::ERROR => Self::Error,
            Level::WARN => Self::Warn,
            Level::INFO => Self::Info,
            Level::DEBUG | Level::TRACE => Self::Debug,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LogStream {
    Stdout,
    Stderr,
}

impl LogStream {
    fn parse(value: &str) -> Option<Self> {
        match value {
            "stdout" => Some(Self::Stdout),
            "stderr" => Some(Self::Stderr),
            _ => None,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Stdout => "stdout",
            Self::Stderr => "stderr",
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct LogEvent {
    pub(crate) timestamp: SystemTime,
    pub(crate) target: LogTarget,
    pub(crate) level: LogLevel,
    pub(crate) task_id: Option<u64>,
    pub(crate) stream: Option<LogStream>,
    pub(crate) fields: Vec<(String, String)>,
    pub(crate) message: String,
}

impl LogEvent {
    pub(crate) fn format_line(&self) -> String {
        let mut context = Vec::new();
        if let Some(task_id) = self.task_id {
            context.push(format!("task_id={task_id}"));
        }
        if let Some(stream) = self.stream {
            context.push(format!("stream={}", stream.as_str()));
        }
        context.extend(
            self.fields
                .iter()
                .map(|(name, value)| format!("{name}={value}")),
        );
        let context = if context.is_empty() {
            String::new()
        } else {
            format!(" [{}]", context.join(" "))
        };
        format_timestamp(self.timestamp)
            + &format!(
                " [{}] [{}]{context} {}",
                self.level.as_str(),
                self.target.as_str(),
                self.message
            )
    }
}

#[derive(Debug)]
enum RouterMessage {
    Event(LogEvent),
    Shutdown,
}

struct RouterLayer {
    sender: Sender<RouterMessage>,
}

impl<S> Layer<S> for RouterLayer
where
    S: Subscriber,
{
    fn on_event(&self, event: &Event<'_>, _context: Context<'_, S>) {
        let mut visitor = EventVisitor::default();
        event.record(&mut visitor);

        let target = LogTarget::parse(event.metadata().target());
        let message = visitor.message.trim().to_owned();
        if message.is_empty() {
            return;
        }

        let event = LogEvent {
            timestamp: SystemTime::now(),
            target,
            level: LogLevel::from_tracing(event.metadata().level()),
            task_id: visitor.task_id,
            stream: visitor.stream,
            fields: visitor.fields,
            message,
        };
        let _ = self.sender.send(RouterMessage::Event(event));
    }
}

#[derive(Default)]
struct EventVisitor {
    message: String,
    task_id: Option<u64>,
    stream: Option<LogStream>,
    fields: Vec<(String, String)>,
}

impl EventVisitor {
    fn record_value(&mut self, field: &tracing::field::Field, value: String) {
        match field.name() {
            "message" => self.message = sanitize_text(&value),
            "task_id" => self.task_id = value.parse().ok(),
            "stream" => self.stream = LogStream::parse(&value),
            name => self
                .fields
                .push((name.to_owned(), sanitize_field(name, &value))),
        }
    }
}

impl tracing::field::Visit for EventVisitor {
    fn record_debug(
        &mut self,
        field: &tracing::field::Field,
        value: &dyn fmt::Debug,
    ) {
        self.record_value(field, debug_value(value));
    }

    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        self.record_value(field, value.to_owned());
    }

    fn record_i64(&mut self, field: &tracing::field::Field, value: i64) {
        self.record_value(field, value.to_string());
    }

    fn record_u64(&mut self, field: &tracing::field::Field, value: u64) {
        self.record_value(field, value.to_string());
    }

    fn record_bool(&mut self, field: &tracing::field::Field, value: bool) {
        self.record_value(field, value.to_string());
    }
}

pub(crate) struct LogRuntime {
    log_dir: PathBuf,
    sender: Sender<RouterMessage>,
    ui_receiver: Receiver<LogEvent>,
    writer: Option<JoinHandle<()>>,
}

impl LogRuntime {
    pub(crate) fn init() -> Self {
        let log_dir = default_log_dir();
        let (sender, receiver) = mpsc::channel();
        let (ui_sender, ui_receiver) = mpsc::sync_channel(UI_QUEUE_CAPACITY);
        let writer_log_dir = log_dir.clone();
        let writer_ui_sender = ui_sender.clone();
        let writer = thread::Builder::new()
            .name("aio-log-writer".to_owned())
            .spawn(move || {
                writer_loop(receiver, writer_ui_sender, writer_log_dir)
            })
            .ok();
        if writer.is_none() {
            let _ = ui_sender.try_send(LogEvent {
                timestamp: SystemTime::now(),
                target: LogTarget::App,
                level: LogLevel::Error,
                task_id: None,
                stream: None,
                fields: Vec::new(),
                message: "Log writer thread is unavailable".to_owned(),
            });
        }

        let filter = EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| EnvFilter::new("info"));
        let subscriber =
            tracing_subscriber::registry()
                .with(filter)
                .with(RouterLayer {
                    sender: sender.clone(),
                });
        let _ = tracing::subscriber::set_global_default(subscriber);

        Self {
            log_dir,
            sender,
            ui_receiver,
            writer,
        }
    }

    pub(crate) fn drain_ui(&self) -> Vec<LogEvent> {
        self.ui_receiver.try_iter().collect()
    }

    pub(crate) fn log_dir(&self) -> &Path {
        &self.log_dir
    }

    fn shutdown(&mut self) {
        let _ = self.sender.send(RouterMessage::Shutdown);
        if let Some(writer) = self.writer.take() {
            let _ = writer.join();
        }
    }
}

impl Drop for LogRuntime {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn writer_loop(
    receiver: Receiver<RouterMessage>,
    ui_sender: SyncSender<LogEvent>,
    log_dir: PathBuf,
) {
    let mut sinks = LogSinks::new(&log_dir, ui_sender.clone());
    while let Ok(message) = receiver.recv() {
        match message {
            RouterMessage::Event(event) => {
                sinks.write_event(&event);
                let _ = ui_sender.try_send(event);
            }
            RouterMessage::Shutdown => break,
        }
    }
    sinks.flush();
}

struct LogSinks {
    log_dir: PathBuf,
    aggregate: Option<RotatingFile>,
    category_sinks: HashMap<&'static str, RotatingFile>,
    failed_categories: HashSet<&'static str>,
    ui_sender: Option<SyncSender<LogEvent>>,
    reported_errors: HashSet<String>,
}

impl LogSinks {
    fn new(log_dir: &Path, ui_sender: SyncSender<LogEvent>) -> Self {
        let mut sinks = Self {
            log_dir: log_dir.to_path_buf(),
            aggregate: None,
            category_sinks: HashMap::new(),
            failed_categories: HashSet::new(),
            ui_sender: Some(ui_sender),
            reported_errors: HashSet::new(),
        };
        sinks.aggregate = sinks.open_sink("debug.log");
        sinks
    }

    fn open_sink(&mut self, file_name: &'static str) -> Option<RotatingFile> {
        match RotatingFile::open(self.log_dir.join(file_name)) {
            Ok(sink) => Some(sink),
            Err(error) => {
                self.report_error(file_name, &error);
                None
            }
        }
    }

    fn write_event(&mut self, event: &LogEvent) {
        let line = event.format_line();
        if let Some(sink) = self.aggregate.as_mut() {
            if let Err(error) = sink.write_line(&line) {
                self.report_error("debug.log", &error);
                self.aggregate = None;
            }
        }

        if let Some(file_name) = event.target.file_name() {
            if !self.failed_categories.contains(file_name)
                && !self.category_sinks.contains_key(file_name)
            {
                if let Some(sink) = self.open_sink(file_name) {
                    self.category_sinks.insert(file_name, sink);
                } else {
                    self.failed_categories.insert(file_name);
                }
            }
            if let Some(sink) = self.category_sinks.get_mut(file_name) {
                if let Err(error) = sink.write_line(&line) {
                    self.report_error(file_name, &error);
                    self.category_sinks.remove(file_name);
                    self.failed_categories.insert(file_name);
                }
            }
        }
    }

    fn report_error(&mut self, file_name: &str, error: &io::Error) {
        let key = format!("{file_name}: {error}");
        if !self.reported_errors.insert(key.clone()) {
            return;
        }
        if let Some(sender) = self.ui_sender.as_ref() {
            let _ = sender.try_send(LogEvent {
                timestamp: SystemTime::now(),
                target: LogTarget::App,
                level: LogLevel::Error,
                task_id: None,
                stream: None,
                fields: Vec::new(),
                message: format!(
                    "Log file {file_name} is unavailable: {error}"
                ),
            });
        }
    }

    fn flush(&mut self) {
        if let Some(sink) = self.aggregate.as_mut() {
            let _ = sink.flush();
        }
        for sink in self.category_sinks.values_mut() {
            let _ = sink.flush();
        }
    }
}

struct RotatingFile {
    path: PathBuf,
    writer: Option<BufWriter<File>>,
    bytes: u64,
    max_bytes: u64,
    max_backups: u32,
}

impl RotatingFile {
    fn open(path: PathBuf) -> io::Result<Self> {
        Self::open_with_limits(path, MAX_LOG_FILE_BYTES, MAX_LOG_BACKUPS)
    }

    fn open_with_limits(
        path: PathBuf,
        max_bytes: u64,
        max_backups: u32,
    ) -> io::Result<Self> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let existing_bytes = fs::metadata(&path)
            .map(|metadata| metadata.len())
            .unwrap_or(0);
        if existing_bytes >= max_bytes {
            rotate_path(&path, max_backups)?;
        }
        let file = OpenOptions::new().create(true).append(true).open(&path)?;
        let bytes = file.metadata()?.len();
        Ok(Self {
            path,
            writer: Some(BufWriter::new(file)),
            bytes,
            max_bytes,
            max_backups,
        })
    }

    fn write_line(&mut self, line: &str) -> io::Result<()> {
        let line_bytes = line.len() as u64 + 1;
        if self.bytes > 0 && self.bytes + line_bytes > self.max_bytes {
            self.rotate()?;
        }
        let writer = self.writer.as_mut().expect("rotating writer is open");
        writer.write_all(line.as_bytes())?;
        writer.write_all(b"\n")?;
        self.bytes += line_bytes;
        Ok(())
    }

    fn rotate(&mut self) -> io::Result<()> {
        if let Some(mut writer) = self.writer.take() {
            writer.flush()?;
        }
        rotate_path(&self.path, self.max_backups)?;
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;
        self.bytes = file.metadata()?.len();
        self.writer = Some(BufWriter::new(file));
        Ok(())
    }

    fn flush(&mut self) -> io::Result<()> {
        if let Some(writer) = self.writer.as_mut() {
            writer.flush()
        } else {
            Ok(())
        }
    }
}

fn rotate_path(path: &Path, max_backups: u32) -> io::Result<()> {
    for index in (1..=max_backups).rev() {
        let source = backup_path(path, index - 1);
        let destination = backup_path(path, index);
        if destination.exists() {
            fs::remove_file(&destination)?;
        }
        if source.exists() {
            fs::rename(source, destination)?;
        }
    }
    Ok(())
}

fn backup_path(path: &Path, index: u32) -> PathBuf {
    if index == 0 {
        return path.to_path_buf();
    }
    let mut value = path.as_os_str().to_os_string();
    value.push(format!(".{index}"));
    PathBuf::from(value)
}

pub(crate) fn default_log_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(LOG_DIRECTORY_NAME)
        .join(LOG_SUBDIRECTORY_NAME)
}

pub(crate) fn next_task_id() -> u64 {
    NEXT_TASK_ID.fetch_add(1, Ordering::Relaxed)
}

pub(crate) fn safe_path_label(path: &Path) -> String {
    let name = path
        .file_name()
        .map(|value| value.to_string_lossy().into_owned())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "<unnamed>".to_owned());
    let mut hasher = Sha256::new();
    hasher.update(path.to_string_lossy().as_bytes());
    let digest = hasher.finalize();
    let short_hash = digest[..4]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    format!("{name}#{short_hash}")
}

fn sanitize_field(name: &str, value: &str) -> String {
    if matches!(name, "path" | "input" | "output" | "blender" | "executable") {
        if value.contains('/') || value.contains('\\') {
            safe_path_label(Path::new(value))
        } else {
            sanitize_text(value)
        }
    } else {
        sanitize_text(value)
    }
}

fn sanitize_text(value: &str) -> String {
    value
        .replace(['\r', '\n'], " ")
        .split_whitespace()
        .map(|part| {
            if part.contains('/') || part.contains('\\') {
                part.rsplit(['/', '\\']).next().unwrap_or(part)
            } else {
                part
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn debug_value(value: &dyn fmt::Debug) -> String {
    let value = format!("{value:?}");
    value
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
        .unwrap_or(&value)
        .to_owned()
}

fn format_timestamp(timestamp: SystemTime) -> String {
    let duration = timestamp
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::ZERO);
    let seconds = duration.as_secs();
    let days = (seconds / 86_400) as i64;
    let seconds_in_day = seconds % 86_400;
    let hour = seconds_in_day / 3_600;
    let minute = (seconds_in_day % 3_600) / 60;
    let second = seconds_in_day % 60;
    let millis = duration.subsec_millis();
    let (year, month, day) = civil_from_days(days);
    format!(
        "{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}.{millis:03}Z"
    )
}

fn civil_from_days(days_since_epoch: i64) -> (i64, i64, i64) {
    let z = days_since_epoch + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let day_of_era = z - era * 146_097;
    let year_of_era = (day_of_era - day_of_era / 1_460 + day_of_era / 36_524
        - day_of_era / 146_096)
        / 365;
    let year = year_of_era + era * 400;
    let day_of_year =
        day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_part = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_part + 2) / 5 + 1;
    let month = month_part + if month_part < 10 { 3 } else { -9 };
    let year = year + i64::from(month <= 2);
    (year, month, day)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_directory(name: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time is after unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "aio-asset-normalizer-{name}-{}-{nonce}",
            std::process::id()
        ))
    }

    #[test]
    fn feature_targets_share_the_retarget_log() {
        assert_eq!(LogTarget::Retarget.file_name(), Some("retarget.log"));
        assert_eq!(LogTarget::GlbRetarget.file_name(), Some("retarget.log"));
        assert_eq!(LogTarget::RetargetAgent.file_name(), Some("retarget.log"));
    }

    #[test]
    fn safe_path_label_does_not_expose_parent_directories() {
        let label = safe_path_label(Path::new("/private/assets/character.glb"));
        assert!(label.starts_with("character.glb#"));
        assert!(!label.contains("private"));
        assert!(!label.contains("assets"));
    }

    #[test]
    fn events_are_written_to_aggregate_and_feature_sinks() {
        let directory = test_directory("routing");
        let (ui_sender, _ui_receiver) = mpsc::sync_channel(8);
        let mut sinks = LogSinks::new(&directory, ui_sender);
        let event = LogEvent {
            timestamp: UNIX_EPOCH,
            target: LogTarget::GlbEditor,
            level: LogLevel::Info,
            task_id: Some(42),
            stream: None,
            fields: Vec::new(),
            message: "Loaded asset".to_owned(),
        };
        sinks.write_event(&event);
        sinks.flush();

        let aggregate = fs::read_to_string(directory.join("debug.log"))
            .expect("aggregate log is written");
        let feature = fs::read_to_string(directory.join("glb-editor.log"))
            .expect("feature log is written");
        assert_eq!(aggregate, feature);
        assert!(aggregate.contains("[INFO] [glb_editor]"));
        assert!(aggregate.contains("task_id=42"));

        fs::remove_dir_all(directory).expect("test directory is removed");
    }

    #[test]
    fn retarget_aliases_are_written_to_one_feature_sink() {
        let directory = test_directory("retarget-routing");
        let (ui_sender, _ui_receiver) = mpsc::sync_channel(8);
        let mut sinks = LogSinks::new(&directory, ui_sender);
        for target in [
            LogTarget::Retarget,
            LogTarget::GlbRetarget,
            LogTarget::RetargetAgent,
        ] {
            sinks.write_event(&LogEvent {
                timestamp: UNIX_EPOCH,
                target,
                level: LogLevel::Info,
                task_id: None,
                stream: None,
                fields: Vec::new(),
                message: "retarget event".to_owned(),
            });
        }
        sinks.flush();

        let feature = fs::read_to_string(directory.join("retarget.log"))
            .expect("retarget log is written");
        assert_eq!(feature.lines().count(), 3);
        assert!(feature.contains("[retarget]"));
        assert!(feature.contains("[glb_retarget]"));
        assert!(feature.contains("[retarget_agent]"));

        fs::remove_dir_all(directory).expect("test directory is removed");
    }

    #[test]
    fn formatted_events_keep_task_and_stream_context() {
        let event = LogEvent {
            timestamp: UNIX_EPOCH,
            target: LogTarget::FbxConverter,
            level: LogLevel::Info,
            task_id: Some(7),
            stream: Some(LogStream::Stderr),
            fields: vec![(
                "input".to_owned(),
                "character.glb#12345678".to_owned(),
            )],
            message: "Blender output".to_owned(),
        };
        let line = event.format_line();
        assert!(line.contains("[fbx_converter]"));
        assert!(line.contains("task_id=7"));
        assert!(line.contains("stream=stderr"));
        assert!(line.contains("character.glb#12345678"));
    }

    #[test]
    fn single_writer_keeps_concurrent_events_as_complete_lines() {
        let directory = test_directory("concurrent");
        let (sender, receiver) = mpsc::sync_channel(512);
        let (ui_sender, _ui_receiver) = mpsc::sync_channel(512);
        let writer_directory = directory.clone();
        let writer = thread::spawn(move || {
            writer_loop(receiver, ui_sender, writer_directory);
        });

        let mut producers = Vec::new();
        for producer in 0..4 {
            let sender = sender.clone();
            producers.push(thread::spawn(move || {
                for event_index in 0..50 {
                    sender
                        .send(RouterMessage::Event(LogEvent {
                            timestamp: UNIX_EPOCH,
                            target: LogTarget::App,
                            level: LogLevel::Info,
                            task_id: Some(producer),
                            stream: None,
                            fields: Vec::new(),
                            message: format!("event-{producer}-{event_index}"),
                        }))
                        .expect("writer is accepting events");
                }
            }));
        }
        for producer in producers {
            producer.join().expect("producer finishes");
        }
        drop(sender);
        writer
            .join()
            .expect("writer finishes after all producers disconnect");

        let contents = fs::read_to_string(directory.join("debug.log"))
            .expect("aggregate log is written");
        let lines = contents.lines().collect::<Vec<_>>();
        assert_eq!(lines.len(), 200);
        assert!(lines.iter().all(|line| {
            line.matches("event-").count() == 1
                && line.starts_with("1970-01-01T00:00:00.000Z [INFO] [app]")
        }));

        fs::remove_dir_all(directory).expect("test directory is removed");
    }

    #[test]
    fn rotation_keeps_only_configured_backup_count() {
        let directory = test_directory("rotation");
        let path = directory.join("debug.log");
        let mut sink =
            RotatingFile::open_with_limits(path.clone(), 16, 3).unwrap();
        for index in 0..12 {
            sink.write_line(&format!("line-{index:02}")).unwrap();
        }
        sink.flush().unwrap();

        assert!(path.exists());
        assert!(backup_path(&path, 1).exists());
        assert!(backup_path(&path, 2).exists());
        assert!(backup_path(&path, 3).exists());
        assert!(!backup_path(&path, 4).exists());

        fs::remove_dir_all(directory).expect("test directory is removed");
    }

    #[test]
    fn unavailable_log_directory_is_reported_without_panicking() {
        let directory = test_directory("unavailable");
        fs::write(&directory, b"not a directory").unwrap();
        let (ui_sender, ui_receiver) = mpsc::sync_channel(8);
        let mut sinks = LogSinks::new(&directory, ui_sender);
        sinks.write_event(&LogEvent {
            timestamp: UNIX_EPOCH,
            target: LogTarget::FbxConverter,
            level: LogLevel::Info,
            task_id: None,
            stream: None,
            fields: Vec::new(),
            message: "event".to_owned(),
        });

        let status = ui_receiver.try_iter().collect::<Vec<_>>();
        assert!(status.iter().any(|event| {
            event.message.contains("debug.log")
                && event.level == LogLevel::Error
        }));

        fs::remove_file(directory).expect("test file is removed");
    }

    #[test]
    fn civil_date_conversion_matches_unix_epoch() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(20_000), (2024, 10, 4));
    }
}
