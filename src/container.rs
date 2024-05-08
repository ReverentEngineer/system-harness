use crate::{Error, ErrorKind, Status, SystemHarness, SystemTerminal};
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
use std::process::{Command, Output, Stdio, Child};

fn strip_last_newline(input: &str) -> &str {
    input
        .strip_suffix("\r\n")
        .or(input.strip_suffix("\n"))
        .unwrap_or(input)
}

/// Process output to result
fn output_to_result(output: Output) -> Result<String, Error> {
    match output.status.success() {
        true => Ok(strip_last_newline(
                std::str::from_utf8(&output.stdout)?
        ).to_string()),
        false => {
            let error = std::str::from_utf8(&output.stderr)?;
            Err(Error::new(ErrorKind::HarnessError, error))
        }
    }
}

/// A container system config
#[derive(Clone, Serialize, Deserialize)]
pub struct ContainerSystemConfig {

    /// Container runtime
    tool: String,

    /// Container image
    image: String,

}

impl ContainerSystemConfig {

    /// Build and run a container based on name
    pub fn build(&self) -> Result<ContainerSystem, Error> {
        let id = Command::new(&self.tool)
            .arg("create")
            .arg("-t") 
            .arg(&self.image)
            .output()
            .map_err(|err| err.into())
            .and_then(output_to_result)
            .map_err(|err| { log::warn!("{err}"); err })?;
        log::trace!("Created container: {id}");

        Command::new(&self.tool)
            .stdout(Stdio::null())
            .arg("start")
            .arg(&id)
            .status()?;

        Ok(ContainerSystem {
            id,
            tool: self.tool.clone()
        })
    }

}

pub struct ContainerSystem {
    tool: String,
    id: String,
}

pub struct ContainerSystemTerminal {
    process: Child
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct State {
    running: bool,
    paused: bool
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Inspect {
    state: State
}

impl SystemTerminal for ContainerSystemTerminal {

    fn send_key(&mut self, _key: crate::Key) -> Result<(), Error> {
        Err(Error::new(ErrorKind::HarnessError, "Sending a keystroke not supported"))
    }

}

impl Read for ContainerSystemTerminal {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.process.stdout.as_mut()
            .ok_or(std::io::Error::new(
                    std::io::ErrorKind::BrokenPipe, 
                    "Can't read from container"
                    ))
            .and_then(|stdout| stdout.read(buf))
    }
}

impl Write for ContainerSystemTerminal {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.process.stdin.as_mut()
            .ok_or(std::io::Error::new(
                    std::io::ErrorKind::BrokenPipe, 
                    "Can't write to container"
                    ))
            .and_then(|stdin| stdin.write(buf))
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.process.stdin.as_mut()
            .ok_or(std::io::Error::new(
                    std::io::ErrorKind::BrokenPipe, 
                    "Can't write to container"
                    ))
            .and_then(|stdin| stdin.flush())
    }
}



impl SystemHarness for ContainerSystem {

    type Terminal = ContainerSystemTerminal;

    fn terminal(&self) -> Result<Self::Terminal, Error> {
        let process = Command::new(&self.tool)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .arg("exec")
            .arg("-it")
            .arg(&self.id)
            .arg("sh")
            .spawn()?;
        Ok(Self::Terminal {
            process
        })
    }

    fn pause(&mut self) -> Result<(), Error> {
        log::trace!("Pausing container: {}", &self.id); 
        Command::new(&self.tool)
            .arg("pause")
            .arg(&self.id)
            .output()
            .map_err(|err| err.into())
            .and_then(output_to_result)
            .map(|_| log::trace!("Paused container: {}", self.id))
    }

    fn resume(&mut self) -> Result<(), Error> {
        log::trace!("Resuming container: {}", &self.id); 
        Command::new(&self.tool)
            .arg("unpause")
            .arg(&self.id)
            .output()
            .map_err(|err| err.into())
            .and_then(output_to_result)
            .map(|_| log::trace!("Resumed container: {}", self.id))
    }

    fn shutdown(&mut self) -> Result<(), Error> {
        log::trace!("Shutting down container: {}", &self.id); 
         Command::new(&self.tool)
            .arg("stop")
            .arg(&self.id)
            .output()
            .map_err(|err| err.into())
            .and_then(output_to_result)
            .map(|_| log::trace!("Stopped container: {}", self.id))
    }

    fn status(&mut self) -> Result<Status, Error> {
        Command::new(&self.tool)
            .arg("inspect")
            .arg(&self.id)
            .output()
            .map_err(|err| err.into())
            .and_then(output_to_result)
            .map_err(|err| { log::warn!("{err}"); err })
            .and_then(|stdout| {
                let inspect: Vec<Inspect> = serde_json::from_str(&stdout)?;
                inspect.into_iter()
                    .next()
                    .ok_or(Error::new(ErrorKind::HarnessError, "Container doesn't exist"))
                    .and_then(|inspect| {
                        let state = &inspect.state;
                        if state.running {
                            Ok(Status::Running)
                        } else if state.paused {
                            Ok(Status::Paused)
                        } else if !state.running && !state.paused {
                            Ok(Status::Shutdown)
                        } else {
                            Err(Error::new(ErrorKind::HarnessError,
                                    format!("Unhandled status")))
                        }
                    })
            })
    }

    fn running(&mut self) -> Result<bool, Error> {
        self.status().map(|status| status == Status::Running)
    }

}

impl Drop for ContainerSystem {
    fn drop(&mut self) {
        if let Ok(running) = self.running() {
            if running {
                if let Ok(()) = self.shutdown() {
                    log::trace!("Deleting container: {}", &self.id); 
                    let _ = Command::new(&self.tool)
                        .args(&["rm", "-f", &self.id])
                        .output();
                } else {
                    log::warn!("Failed to shutdown: {}", &self.id);
                }
            }
        }
    }
}
