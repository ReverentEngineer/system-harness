use crate::qemu::args::PropertyValue;
use cmdstruct::Arg;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use system_harness_macros::{Backend, PropertyList};

#[derive(Clone, Serialize, Deserialize, PropertyList)]
#[serde(rename_all = "kebab-case")]
pub struct Boot {
    menu: Option<OnOff>,
    strict: Option<OnOff>,
    #[serde(rename = "reboot-time")]
    reboot_time: Option<String>,
    #[serde(rename = "splash-time")]
    splash_time: Option<String>,
    splash: Option<String>,
    once: Option<String>,
    order: Option<String>
}

#[derive(Copy, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Discard {
    Ignore,
    Unmap,
}

impl PropertyValue for Discard {
    fn value(&self) -> Option<String> {
        match self {
            Discard::Ignore => Some(String::from("ignore")),
            Discard::Unmap => Some(String::from("unmap")),
        }
    }
}

#[derive(Clone, Serialize, Deserialize, PropertyList)]
#[serde(rename_all = "kebab-case")]
pub struct BlockDev {
    /// Block device driver
    driver: String,

    /// Block node name
    #[serde(rename = "node-name")]
    node_name: String,

    /// Discard strategy
    discard: Option<Discard>,

    #[serde(flatten)]
    properties: BTreeMap<String, String>,
}

#[derive(Serialize, Deserialize)]
pub struct Backend<T> {
    backend: T,
    id: String,
}

impl<T> Arg for Backend<T>
where
    T: super::args::Backend,
{
    fn append_arg(&self, command: &mut std::process::Command) {
        command.arg(format!(
            "{},id={},{}",
            self.backend.name(),
            self.id,
            self.backend.properties()
        ));
    }
}

impl<T: Clone> Clone for Backend<T> {
    fn clone(&self) -> Self {
        Self {
            backend: self.backend.clone(),
            id: self.id.clone()
        }
    }
}

#[derive(Clone, Serialize, Deserialize, Backend)]
#[serde(rename_all = "kebab-case")]
pub enum CharDev {
    Stdio,
    Socket { path: String },
}

#[derive(Copy, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum OnOff {
    On,
    Off
}

impl PropertyValue for OnOff {
    fn value(&self) -> Option<String> {
        match self {
            OnOff::On => Some(String::from("on")),
            OnOff::Off => Some(String::from("off")),
        }
    }
}

#[derive(Clone, Serialize, Deserialize, Backend)]
#[serde(rename_all = "kebab-case")]
pub enum NetDev {
    User {
        ipv4: OnOff,

        net: String,

        host: String
    },
}

#[derive(Clone, Serialize, Deserialize, PropertyList)]
pub struct Device {
    /// Device driver
    driver: String,

    /// Device driver properties
    #[serde(flatten)]
    properties: BTreeMap<String, String>,
}

#[derive(Clone, Serialize, Deserialize, PropertyList)]
pub struct Smp {
    /// Number of CPUs
    cpus: Option<usize>,

    /// Maximum CPUs
    maxcpus: Option<usize>,

    /// Number of dies
    dies: Option<usize>,

    /// Number of sockets
    sockets: Option<usize>,

    /// Number of clusters
    clusters: Option<usize>,

    /// Number of cores
    cores: Option<usize>,

    /// Number of threads
    threads: Option<usize>,
}

#[derive(Clone, Serialize, Deserialize, PropertyList)]
pub struct Machine {
    /// Machine type
    #[serde(rename = "type")]
    r#type: Option<String>,

    #[serde(flatten)]
    properties: BTreeMap<String, String>,
}

#[cfg(test)]
mod tests {

    use super::*;
    use crate::qemu::args::Backend as _;
    use cmdstruct::Arg;

    #[test]
    fn chardev() {
        const EXPECTED: &'static str = r#"{"backend":{"socket":{"path":"test.sock"}},"id":"abc"}"#;
        let chardev = Backend::<CharDev> {
            id: "abc".to_string(),
            backend: CharDev::Socket {
                path: "test.sock".to_string(),
            },
        };
        assert_eq!("socket", chardev.backend.name());
        assert_eq!(
            "path=test.sock",
            format!("{}", chardev.backend.properties())
        );
        assert_eq!(EXPECTED, &serde_json::to_string(&chardev).unwrap());
    }

    #[test]
    fn device_arg() {
        let mut properties = BTreeMap::new();
        properties.insert("a".to_string(), "abc".to_string());
        let device = Device {
            driver: "test".to_string(),
            properties,
        };
        let mut command = std::process::Command::new("test");
        device.append_arg(&mut command);
        assert_eq!(
            vec!["driver=test,a=abc"],
            command.get_args().collect::<Vec<_>>()
        );
    }
}
