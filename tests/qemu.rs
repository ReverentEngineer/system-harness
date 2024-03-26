extern crate system_harness;

use std::sync::{Arc, Mutex};
use system_harness::{Event, EventKind, EventPublisher, QemuSystemConfig, SystemHarness};

const JSON_CONFIG: &'static str = include_str!("../tests/data/qemu-config.json");

#[derive(Default, Debug)]
struct Events {
    pause: usize,
    resume: usize,
    shutdown: usize,
}

#[test_log::test]
fn build() {
    let config: QemuSystemConfig = serde_json::from_str(JSON_CONFIG).unwrap();
    let mut system = config.build().unwrap();
    let events = Arc::new(Mutex::new(Events::default()));
    {
        let events = events.clone();
        system
            .subscribe(move |event: &Event| {
                let mut guard = events.lock().unwrap();
                match event.kind {
                    EventKind::Shutdown => guard.shutdown += 1,
                    EventKind::Resume => guard.resume += 1,
                    EventKind::Pause => guard.pause += 1,
                    EventKind::Suspend => {}
                }
            })
            .unwrap();
    }
    assert!(system.running().unwrap());
    assert_eq!(system.status().unwrap(), system_harness::Status::Running);
    system.pause().unwrap();
    assert_eq!(system.status().unwrap(), system_harness::Status::Paused);
    system.resume().unwrap();
    assert_eq!(system.status().unwrap(), system_harness::Status::Running);
    assert!(system.running().unwrap());
    system.shutdown().unwrap();
    drop(system);
    let guard = events.lock().unwrap();
    let pauses = guard.pause;
    let resumes = guard.resume;
    let shutdowns = guard.shutdown;
    drop(guard);
    assert_eq!(pauses, 1);
    assert_eq!(resumes, 1);
    assert_eq!(shutdowns, 1);
}
