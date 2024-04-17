#![doc = include_str!("../README.md")]
//!
//! # QEMU
//! 
//! A [`QemuSystem`](`crate::QemuSystem`) that implements 
//! [`SystemHarness`](`crate::SystemHarness`) can be instantiated using a 
//! [`QemuSystemConfig`](`crate::QemuSystemConfig`) that can be deserialized
//! using serde.
//!
//! The top-most mapping should align with the QEMU argument names with the
//! sub-mappings aligning with the backends and/or properties of the arguments.
//!
//! An example QEMU configuration:
//!```json
#![doc = include_str!("../tests/data/qemu-config.json")]
//!```
//! # Containers
//!
//! A [`ContainerSystem`](`crate::ContainerSystem`) that implements 
//! [`SystemHarness`](`crate::SystemHarness`) can be instantiated using a 
//! [`ContainerSystemConfig`](`crate::ContainerSystemConfig`) that can be deserialized
//! using serde.
//!
//! An example of a container configuration:
//!```json
#![doc = include_str!("../tests/data/container-config.json")]
//!```
use std::io::{Read, Write};
use std::time::SystemTime;

/// System keyboard key
#[derive(Debug, PartialEq)]
pub enum Key {
    Enter,
}

/// System status
#[derive(Debug, PartialEq)]
pub enum Status {
    Running,
    Paused,
    Suspended,
    Shutdown,
}

/// Type of event
#[derive(Debug, PartialEq)]
pub enum EventKind {
    Shutdown,
    Resume,
    Pause,
    Suspend,
}

/// A machine event
pub struct Event {
    /// Type of event
    pub kind: EventKind,

    /// Time event occurred
    pub timestamp: SystemTime,
}

/// A trait representing event listener
pub trait EventSubscriber: Send + Sync + 'static {
    /// Action to be performed on event
    fn on_event(&mut self, event: &Event);
}

/// A trait representing a harnessed system
pub trait SystemHarness: Write + Read {
    /// Send key to emulator
    fn send_key(&mut self, key: Key) -> Result<(), Error>;

    /// Pause system
    fn pause(&mut self) -> Result<(), Error>;

    /// Resume system
    fn resume(&mut self) -> Result<(), Error>;

    /// Shutdown system
    fn shutdown(&mut self) -> Result<(), Error>;

    /// Get system status
    fn status(&mut self) -> Result<Status, Error>;

    /// Check if harness is running
    fn running(&mut self) -> Result<bool, Error>;
}

/// An event publisher
pub trait EventPublisher {
    /// Subscribe event listener
    fn subscribe(&mut self, subscriber: impl EventSubscriber) -> Result<(), Error>;
}

impl<F> EventSubscriber for F
where
    F: FnMut(&Event) + Send + Sync + 'static,
{
    fn on_event(&mut self, event: &Event) {
        (self)(event)
    }
}

mod error;
pub use error::Error;
pub use error::ErrorKind;

#[cfg(all(target_family = "unix", feature = "container"))]
mod container;
#[cfg(all(target_family = "unix", feature = "container"))]
pub use container::*;

#[cfg(all(target_family = "unix", feature = "qemu"))]
mod qemu;
#[cfg(all(target_family = "unix", feature = "qemu"))]
pub use qemu::*;

#[cfg(test)]
mod tests {

    use super::*;

    struct FakeEventPublisher(Vec<Box<dyn EventSubscriber>>);

    impl FakeEventPublisher {
        pub fn publish(&mut self) {
            let event = Event {
                kind: EventKind::Shutdown,
                timestamp: SystemTime::now(),
            };
            for subscriber in &mut self.0 {
                subscriber.on_event(&event);
            }
        }
    }

    impl EventPublisher for FakeEventPublisher {
        fn subscribe(&mut self, subscriber: impl EventSubscriber) -> Result<(), Error> {
            self.0.push(Box::new(subscriber));
            Ok(())
        }
    }

    #[test]
    fn fn_subscribe() {
        let mut publisher = FakeEventPublisher(Vec::new());
        publisher.subscribe(|_event: &Event| {}).unwrap();
        publisher.publish();
    }
}
