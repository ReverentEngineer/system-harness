use crate::{Error, ErrorKind, EventPublisher, EventSubscriber, Key, Status, SystemHarness, SystemTerminal};
use cmdstruct::Command;
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::process::Child;

mod args;

mod models;
use models::*;

mod qmp;
use qmp::QmpStream;

fn qemu_system_bin(config: &QemuSystemConfig) -> String {
    format!("qemu-system-{}", config.arch)
}

/// A configuration for running QEMU
///
/// This config can be serialized and deserialized using
/// serde.
#[derive(Clone, Command, Serialize, Deserialize)]
#[command(executable_fn = qemu_system_bin)]
pub struct QemuSystemConfig {
    arch: String,

    #[arg(option = "-boot")]
    boot: Option<Boot>,

    #[arg(option = "-cpu")]
    cpu: Option<String>,

    #[arg(option = "-machine")]
    machine: Option<Machine>,

    #[arg(option = "-smp")]
    smp: Option<Smp>,

    #[arg(option = "-accel")]
    accel: Option<String>,

    #[arg(option = "-bios")]
    bios: Option<String>,

    #[arg(option = "-m")]
    memory: Option<usize>,

    #[arg(option = "-cdrom")]
    cdrom: Option<String>,

    #[arg(option = "-hda")]
    hda: Option<String>,

    #[arg(option = "-hdb")]
    hdb: Option<String>,

    #[arg(option = "-device")]
    device: Option<Vec<Device>>,

    #[arg(option = "-chardev")]
    chardev: Option<Vec<Backend<CharDev>>>,

    #[arg(option = "-netdev")]
    netdev: Option<Vec<Backend<NetDev>>>,

    #[arg(option = "-blockdev")]
    blockdev: Option<Vec<BlockDev>>,

    /// Extra QEMU args
    extra_args: Option<Vec<String>>
}

impl QemuSystemConfig {
    pub fn build(&self) -> Result<QemuSystem, Error> {
        let mut command = self.command();

        command.arg("-nographic");
        command.args(["-qmp", "unix:qmp.sock,server=on,wait=off"]);
        command.args(["-serial", "unix:serial.sock,server=on,wait=off"]);

        if let Some(extra_args) = &self.extra_args {
            command.args(extra_args);
        }

        log::trace!("Starting system...");
        let mut process = command.spawn()?;

        log::trace!("Connecting to QMP socket...");
        let mut qmp_socket = None;
        while process.try_wait()?.is_none() && qmp_socket.is_none() {
            qmp_socket = UnixStream::connect("qmp.sock").ok();
        }
        let qmp = QmpStream::new(qmp_socket.unwrap())?;
        log::trace!("Connecting to serial socket...");
        let serial = UnixStream::connect("serial.sock")?;
        log::trace!("System ready.");
        Ok(QemuSystem {
            process,
            serial,
            qmp,
        })
    }
}

/// A running QEMU system
pub struct QemuSystem {
    process: Child,
    serial: UnixStream,
    qmp: QmpStream,
}

pub struct QemuSystemTerminal {
    serial: UnixStream,
    qmp: QmpStream
}

impl Read for QemuSystemTerminal {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.serial.read(buf)
    }
}

impl Write for QemuSystemTerminal {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.serial.write(buf)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.serial.flush()
    }
}

impl SystemTerminal for QemuSystemTerminal {

    fn send_key(&mut self, key: Key) -> Result<(), Error> {
        self.qmp
            .send_command(qmp::QmpCommand::SendKey(qmp::KeyCommand {
                keys: vec![key.into()],
            }))
            .map(|_| ())
    }

}

impl SystemHarness for QemuSystem {

    type Terminal = QemuSystemTerminal;

    fn terminal(&self) -> Result<Self::Terminal, Error> {
        let serial = self.serial.try_clone()?;
        let qmp = self.qmp.try_clone()?;
        Ok(QemuSystemTerminal {
            serial,
            qmp
        })
    }


    fn running(&mut self) -> Result<bool, Error> {
        self.process
            .try_wait()
            .map(|status| status == None)
            .map_err(|err| err.into())
    }

    fn pause(&mut self) -> Result<(), Error> {
        self.qmp.send_command(qmp::QmpCommand::Stop).map(|_| ())
    }

    fn resume(&mut self) -> Result<(), Error> {
        self.qmp.send_command(qmp::QmpCommand::Cont).map(|_| ())
    }

    fn shutdown(&mut self) -> Result<(), Error> {
        self.qmp
            .send_command(qmp::QmpCommand::SystemPowerdown)
            .map(|_| ())
    }

    fn status(&mut self) -> Result<Status, Error> {
        self.qmp
            .send_command(qmp::QmpCommand::QueryStatus)
            .and_then(|ret| match ret {
                qmp::QmpReturn::StatusInfo(status) => status.try_into(),
                _ => Err(Error::new(
                    ErrorKind::HarnessError,
                    format!("Unexpected return"),
                )),
            })
    }
}

impl EventPublisher for QemuSystem {
    fn subscribe(&mut self, subscriber: impl EventSubscriber) -> Result<(), Error> {
        self.qmp.subscribe(subscriber)
    }
}

impl Drop for QemuSystem {
    fn drop(&mut self) {
        if let Ok(true) = self.running() {
            log::trace!("Stopping running system...");
            if let Err(err) = self.qmp.send_command(qmp::QmpCommand::Quit) {
                log::warn!("Error quiting system: {err}");
            }
        }
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn json_config() {
        const JSON_CONFIG: &'static str = include_str!("../tests/data/qemu-config.json");
        let config: QemuSystemConfig = serde_json::from_str(JSON_CONFIG).unwrap();
        let command = config.command();
        assert_eq!("qemu-system-i386", command.get_program());
        assert_eq!(
            vec!["-machine", "type=q35", "-m", "512",
                "-device", "driver=virtio-blk,drive=f1",
                "-blockdev", "driver=file,node-name=f1,filename=tests/data/test.raw"],
            command.get_args().collect::<Vec<_>>()
        );
    }
}
