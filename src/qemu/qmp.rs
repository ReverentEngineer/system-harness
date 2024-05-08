#![allow(dead_code)]
use crate::{Error, ErrorKind, Event, EventKind, EventPublisher, EventSubscriber, Key, Status};
use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader, Write};
use std::iter::FromIterator;
use std::os::unix::net::UnixStream;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub struct QmpStream {
    stream: BufReader<UnixStream>,
    version: QemuVersion,
    subscribers: Vec<Box<dyn EventSubscriber>>,
}

pub fn read_message<D>(stream: &mut BufReader<UnixStream>) -> Result<D, Error>
where
    D: for<'de> serde::Deserialize<'de>,
{
    let mut line = String::new();
    stream.read_line(&mut line)?;
    line.truncate(line.len() - 1);
    log::trace!("Received response: {line}");
    serde_json::from_str(&line).map_err(|err| Error::new(ErrorKind::HarnessError, err))
}

fn create_event(timestamp: QmpTimestamp, event: String) -> Option<Event> {
    log::trace!("Saw {event} event");
    match event.as_ref() {
        "POWERDOWN" => Some(EventKind::Shutdown),
        "STOP" => Some(EventKind::Pause),
        "RESUME" => Some(EventKind::Resume),
        _ => None,
    }
    .map(|kind| Event {
        timestamp: timestamp.into(),
        kind,
    })
}

impl QmpStream {
    /// Create new connection QMP
    pub fn new(stream: UnixStream) -> Result<Self, Error> {
        let mut wrapped_stream = BufReader::new(stream);
        let caps: Capabilities = read_message(&mut wrapped_stream)?;
        let mut qmp_stream = Self {
            stream: wrapped_stream,
            version: caps.qmp.version.qemu,
            subscribers: Vec::new(),
        };
        qmp_stream.send_command(QmpCommand::QmpCapabilities)?;
        Ok(qmp_stream)
    }

    pub fn try_clone(&self) -> Result<Self, Error> {
        let stream = self.stream.get_ref().try_clone()?;
        Ok(Self {
            stream: BufReader::new(stream),
            version: self.version,
            subscribers: Vec::new()
        })
    }

    fn send_event(&mut self, event: &Event) -> Result<(), Error> {
        for subscriber in &mut self.subscribers {
            subscriber.on_event(&event);
        }
        Ok(())
    }

    fn wait_for_return(&mut self) -> Result<QmpReturn, Error> {
        loop {
            let response: QmpResponse = read_message(&mut self.stream)?;
            match response {
                QmpResponse::Success { return_data } => return Ok(return_data),
                QmpResponse::Event { timestamp, event } => {
                    if let Some(event) = create_event(timestamp, event) {
                        self.send_event(&event)?;
                    }
                }
                QmpResponse::Error { error } => {
                    return Err(Error::new(ErrorKind::HarnessError, error))
                }
            }
        }
    }

    /// Send QMP command
    pub fn send_command(&mut self, command: QmpCommand) -> Result<QmpReturn, Error> {
        let message = serde_json::to_string(&command)
            .map_err(|err| Error::new(ErrorKind::HarnessError, err))?;
        log::trace!("Sending command: {message}");
        self.stream
            .get_mut()
            .write_all(message.as_bytes())
            .map_err(|err| Error::new(ErrorKind::HarnessError, err))?;
        self.wait_for_return()
    }
}

impl EventPublisher for QmpStream {
    fn subscribe(&mut self, subscriber: impl EventSubscriber) -> Result<(), Error> {
        log::trace!("Subscribing events...");
        self.subscribers.push(Box::new(subscriber));
        Ok(())
    }
}

#[derive(Copy, Clone, Debug, Deserialize)]
pub struct QemuVersion {
    major: usize,
    minor: usize,
    micro: usize,
}

#[derive(Deserialize)]
struct QmpVersion {
    qemu: QemuVersion,
    package: String,
}

#[derive(Deserialize)]
struct QmpCapabilities {
    version: QmpVersion,
    capabilities: Vec<String>,
}

#[derive(Deserialize)]
struct Capabilities {
    #[serde(rename = "QMP")]
    qmp: QmpCapabilities,
}

#[derive(Serialize)]
pub struct KeyCommand {
    pub keys: Vec<KeyValue>,
}

impl FromIterator<Key> for KeyCommand {
    fn from_iter<T: IntoIterator<Item = Key>>(iter: T) -> Self {
        let mut keys = Vec::new();
        for key in iter {
            keys.push(key.into());
        }
        KeyCommand { keys }
    }
}

#[derive(Serialize)]
#[serde(tag = "execute", content = "arguments", rename_all = "kebab-case")]
pub enum QmpCommand {
    #[serde(rename = "qmp_capabilities")]
    QmpCapabilities,
    SendKey(KeyCommand),
    QueryStatus,
    Stop,
    Cont,
    Quit,
    #[serde(rename = "system_powerdown")]
    SystemPowerdown,
}

#[derive(Deserialize, Debug)]
pub struct QmpStatusInfo {
    /// If all vCPUs are runnable.
    running: bool,

    /// If vCPUs are in single step mode
    singlestep: bool,

    /// The run state of the system
    status: String,
}

impl TryInto<Status> for QmpStatusInfo {
    type Error = Error;

    fn try_into(self) -> Result<Status, Self::Error> {
        match self.status.as_ref() {
            "running" => Ok(Status::Running),
            "shutdown" => Ok(Status::Shutdown),
            "paused" => Ok(Status::Paused),
            "save-vm" => Ok(Status::Paused),
            err => Err(Error::new(
                ErrorKind::HarnessError,
                format!("Unsupported status: {err}"),
            )),
        }
    }
}

#[derive(Deserialize, Debug)]
pub struct QmpEmptyReturn {}

#[derive(Deserialize, Debug)]
#[serde(untagged)]
pub enum QmpReturn {
    StatusInfo(QmpStatusInfo),
    Empty(QmpEmptyReturn),
}

#[derive(Deserialize, Debug)]
pub struct QmpTimestamp {
    seconds: u64,
    microseconds: u64,
}

impl From<QmpTimestamp> for SystemTime {
    fn from(value: QmpTimestamp) -> Self {
        let microseconds = (value.seconds * 1000000) + value.microseconds;
        let timestamp = Duration::from_micros(microseconds);
        UNIX_EPOCH + timestamp
    }
}

#[derive(Deserialize)]
#[serde(untagged)]
pub enum QmpResponse {
    Success {
        #[serde(rename = "return")]
        return_data: QmpReturn,
    },
    Error {
        error: String,
    },
    Event {
        timestamp: QmpTimestamp,
        event: String,
    },
}

pub enum KeyValueKind {
    Qcode,
    Number,
}

#[derive(Serialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum KeyValue {
    Qcode { data: &'static str },
    Number { data: usize },
}

impl From<Key> for KeyValue {
    fn from(value: Key) -> Self {
        match value {
            Key::Enter => KeyValue::Qcode { data: "ret" },
        }
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn serialize_send_key() {
        const EXPECTED_COMMAND: &'static str =
            r#"{"execute":"send-key","arguments":{"keys":[{"type":"qcode","data":"ret"}]}}"#;
        let command = QmpCommand::SendKey(KeyCommand {
            keys: vec![Key::Enter.into()],
        });
        let actual = serde_json::to_string(&command).unwrap();
        assert_eq!(EXPECTED_COMMAND, actual);
    }

    #[test]
    fn serialize_quit() {
        const EXPECTED_COMMAND: &'static str = r#"{"execute":"quit"}"#;
        let actual = serde_json::to_string(&QmpCommand::Quit).unwrap();
        assert_eq!(EXPECTED_COMMAND, actual);
    }
}
