extern crate system_harness;

use system_harness::{ContainerSystemConfig, SystemHarness};

const JSON_CONFIG: &'static str = include_str!("../tests/data/container-config.json");

#[test_log::test]
fn build() {
    let config: ContainerSystemConfig = serde_json::from_str(JSON_CONFIG).unwrap();
    let mut system = config.build().unwrap();
    while !system.running().unwrap() {
        // Wait for system to be running
    }
    assert_eq!(system.status().unwrap(), system_harness::Status::Running);
    system.pause().unwrap();
    assert_eq!(system.status().unwrap(), system_harness::Status::Paused);
    system.resume().unwrap();
    assert_eq!(system.status().unwrap(), system_harness::Status::Running);
    assert!(system.running().unwrap());
    system.shutdown().unwrap();
    drop(system);
}
