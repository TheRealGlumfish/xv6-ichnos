use num_enum::TryFromPrimitive;
use std::{fmt, io};
use tokio_util::bytes::Buf;

/// The size of a `uproc` C struct in bytes.
pub const UPROC_SZ: usize = 40;

#[allow(clippy::upper_case_acronyms)] // TODO: Decide on the naming convention for these "C" types
#[derive(TryFromPrimitive, Debug)]
#[repr(i32)]
enum ProcstateEnum {
    UNUSED,
    USED,
    SLEEPING,
    RUNNABLE,
    RUNNING,
    ZOMBIE,
}

/// A xv6 process' state.
#[derive(Clone, Debug)]
pub enum ProcessState {
    Sleeping,
    Runnable,
    Running,
    Zombie(i32),
}

impl fmt::Display for ProcessState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ProcessState::Sleeping => write!(f, "Sleeping"),
            ProcessState::Runnable => write!(f, "Runnable"),
            ProcessState::Running => write!(f, "Running"),
            ProcessState::Zombie(exit_code) => write!(f, "Zombie ({})", exit_code),
        }
    }
}

// TODO: Review visibility of fields in this struct
/// A xv6 process' basic information.
#[derive(Clone, Debug)]
pub struct Process {
    pub state: ProcessState,
    pub pid: i32,
    pub ppid: i32,
    pub sz: u64,
    pub name: String, // TODO: Switch to Rc or Arc for efficient cloning
}

/// A conversion of an xv6 `uproc` C struct in bytes to a `Process` struct.
impl TryFrom<&[u8]> for Process {
    type Error = io::Error; // TODO: Switch to custom error type

    fn try_from(mut frame: &[u8]) -> io::Result<Process> {
        if frame.len() == UPROC_SZ {
            let state_val = frame.get_i32_le();
            let xstate = frame.get_i32_le();
            Ok(Process {
                state: match ProcstateEnum::try_from(state_val)
                    .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err.to_string()))?
                {
                    state @ (ProcstateEnum::UNUSED | ProcstateEnum::USED) => {
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidData,
                            format!("Invalid process state: {:?}", state),
                        ));
                    }
                    ProcstateEnum::SLEEPING => ProcessState::Sleeping,
                    ProcstateEnum::RUNNABLE => ProcessState::Runnable,
                    ProcstateEnum::RUNNING => ProcessState::Running,
                    ProcstateEnum::ZOMBIE => ProcessState::Zombie(xstate),
                },
                pid: frame.get_i32_le(),
                ppid: frame.get_i32_le(),
                sz: frame.get_u64_le(),
                name: std::str::from_utf8(frame.split(|&b| b == b'\0').next().unwrap_or(&[]))
                    .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err.to_string()))?
                    .to_string(),
            })
        } else {
            Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Invalid process frame length: {:?}", frame.len()),
            ))
        }
    }
}
