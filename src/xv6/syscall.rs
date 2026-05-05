use num_enum::TryFromPrimitive;
use strum::{EnumIter, IntoStaticStr};
use tokio_util::bytes::Buf;

use std::io;

/// Size of a `syscall_event` C struct in bytes.
pub const SYSCALL_EVENT_SZ: usize = 64;
/// Syscall trace mask with all system calls disabled.
pub const DISABLE_ALL_MASK: u32 = 0;
/// Syscall trace mask with all system calls enabled.
pub const ENABLE_ALL_MASK: u32 = {
    let mut mask = 0;
    let mut i = 1;
    while i <= 24 {
        mask |= 1 << i;
        i += 1;
    }
    mask
};

const SBRK_EAGER: i32 = 1;
const SBRK_LAZY: i32 = 2;

#[allow(non_camel_case_types)] // TODO: Decide on whether to remove this or rename
#[derive(TryFromPrimitive, IntoStaticStr, EnumIter, Copy, Clone)]
#[repr(i32)]
pub enum SyscallNum {
    SYS_fork = 1,
    SYS_exit = 2,
    SYS_wait = 3,
    SYS_pipe = 4,
    SYS_read = 5,
    SYS_kill = 6,
    SYS_exec = 7,
    SYS_fstat = 8,
    SYS_chdir = 9,
    SYS_dup = 10,
    SYS_getpid = 11,
    SYS_sbrk = 12,
    SYS_pause = 13,
    SYS_uptime = 14,
    SYS_open = 15,
    SYS_write = 16,
    SYS_mknod = 17,
    SYS_unlink = 18,
    SYS_link = 19,
    SYS_mkdir = 20,
    SYS_close = 21,
    SYS_pstat = 22,
    SYS_trace = 23,
    SYS_gettrace = 24,
}

impl SyscallNum {
    /// Get the trace mask corresponding to this system call number.
    pub fn mask(&self) -> u32 {
        1 << (*self as u32)
    }

    /// Returns the "name" of the system call.
    pub fn name(&self) -> &'static str {
        let name: &'static str = self.into();
        &name[4..]
    }

    /// Returns whether the system call is traced based on the given trace mask.
    pub fn is_traced(&self, trace_mask: u32) -> bool {
        (trace_mask & self.mask()) != 0
    }

    /// Returns a new trace mask with this system call enabled.
    pub fn enable_in_mask(&self, trace_mask: u32) -> u32 {
        trace_mask | self.mask()
    }

    /// Returns a new trace mask with this system call disabled.
    pub fn disable_in_mask(&self, trace_mask: u32) -> u32 {
        trace_mask & !self.mask()
    }

    /// Return description of the system call.
    /// Descriptions are taken from the [xv6 book](https://pdos.csail.mit.edu/6.828/2025/xv6/book-riscv-rev5.pdf).
    pub const fn description(&self) -> &'static str {
        match self {
            SyscallNum::SYS_fork => "Create a process, return child’s PID.",
            SyscallNum::SYS_exit => {
                "Terminate the current process; status reported to wait(). No return."
            }
            SyscallNum::SYS_wait => {
                "Wait for a child to exit; exit status in *status; returns child PID."
            }
            SyscallNum::SYS_kill => "Terminate process PID. Returns 0, or -1 for error.",
            SyscallNum::SYS_getpid => "Return the current process’s PID.",
            SyscallNum::SYS_pause => "Pause for n clock ticks.",
            SyscallNum::SYS_exec => {
                "Load a file and execute it with arguments; only returns if error."
            }
            SyscallNum::SYS_sbrk => {
                "Grow process’s memory by n zero bytes. Returns start of new memory."
            } // TODO: Explain the different behind the lazy and eager versions of sbrk
            SyscallNum::SYS_open => {
                "Open a file; flags indicate read/write; returns an fd (file descriptor)."
            }
            SyscallNum::SYS_write => "Write n bytes from buf to file descriptor fd; returns n.",
            SyscallNum::SYS_read => {
                "Read n bytes into buf; returns number read; or 0 if end of file."
            }
            SyscallNum::SYS_close => "Release open file fd.",
            SyscallNum::SYS_dup => "Return a new file descriptor referring to the same file as fd.",
            SyscallNum::SYS_pipe => {
                "Create a pipe, put read/write file descriptors in p[0] and p[1]."
            }
            SyscallNum::SYS_chdir => "Change the current directory.",
            SyscallNum::SYS_mkdir => "Create a new directory.",
            SyscallNum::SYS_mknod => "Create a device file.",
            SyscallNum::SYS_fstat => "Place info about an open file into *st.",
            SyscallNum::SYS_link => "Create another name (file2) for the file file1.",
            SyscallNum::SYS_unlink => "Remove a file",
            SyscallNum::SYS_uptime => {
                "Return how many clock tick interrupts have occurred since start."
            } // TODO: Actually check this
            // TODO: Revise these
            SyscallNum::SYS_pstat => "Place info about the running processes into *buf.",
            SyscallNum::SYS_trace => "Set the trace mask for a process.",
            SyscallNum::SYS_gettrace => "Put up to n trace events from a process into *buf.",
        }
    }
}

impl From<&Syscall> for SyscallNum {
    fn from(value: &Syscall) -> Self {
        match value {
            Syscall::Fork { .. } => SyscallNum::SYS_fork,
            Syscall::Exit { .. } => SyscallNum::SYS_exit,
            Syscall::Wait { .. } => SyscallNum::SYS_wait,
            Syscall::Pipe { .. } => SyscallNum::SYS_pipe,
            Syscall::Read { .. } => SyscallNum::SYS_read,
            Syscall::Kill { .. } => SyscallNum::SYS_kill,
            Syscall::Exec { .. } => SyscallNum::SYS_exec,
            Syscall::Fstat { .. } => SyscallNum::SYS_fstat,
            Syscall::Chdir { .. } => SyscallNum::SYS_chdir,
            Syscall::Dup { .. } => SyscallNum::SYS_dup,
            Syscall::Getpid { .. } => SyscallNum::SYS_getpid,
            Syscall::Sbrk { .. } | Syscall::SbrkLazy { .. } => SyscallNum::SYS_sbrk,
            Syscall::Pause { .. } => SyscallNum::SYS_pause,
            Syscall::Uptime { .. } => SyscallNum::SYS_uptime,
            Syscall::Open { .. } => SyscallNum::SYS_open,
            Syscall::Write { .. } => SyscallNum::SYS_write,
            Syscall::Mknod { .. } => SyscallNum::SYS_mknod,
            Syscall::Unlink { .. } => SyscallNum::SYS_unlink,
            Syscall::Link { .. } => SyscallNum::SYS_link,
            Syscall::Mkdir { .. } => SyscallNum::SYS_mkdir,
            Syscall::Close { .. } => SyscallNum::SYS_close,
            Syscall::PStat { .. } => SyscallNum::SYS_pstat,
            Syscall::Trace { .. } => SyscallNum::SYS_trace,
            Syscall::GetTrace { .. } => SyscallNum::SYS_gettrace,
        }
    }
}

/// A xv6 system call trace buffer entry.
/// Corresponds to a trace buffer entry in xv6, thus it contains the system call number, all the possible arguments (even if not all syscalls use all 6 arguments), and the return value.
pub struct SyscallEvent {
    num: SyscallNum,
    args: [u64; 6],
    retval: u64,
}

/// A conversion of an xv6 `syscall_event` C struct in bytes to a `SyscallEvent`.
impl TryFrom<&[u8]> for SyscallEvent {
    type Error = io::Error; // TODO: Switch to custom error type

    fn try_from(mut frame: &[u8]) -> io::Result<SyscallEvent> {
        if frame.len() == SYSCALL_EVENT_SZ {
            let num_val = frame.get_i32_le();
            frame.advance(4); // 4 bytes of padding after num
            Ok(SyscallEvent {
                num: SyscallNum::try_from(num_val)
                    .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err.to_string()))?,
                args: [
                    frame.get_u64_le(),
                    frame.get_u64_le(),
                    frame.get_u64_le(),
                    frame.get_u64_le(),
                    frame.get_u64_le(),
                    frame.get_u64_le(),
                ],
                retval: frame.get_u64_le(),
            })
        } else {
            Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Invalid syscall event frame length {}", frame.len()),
            ))
        }
    }
}

/// A xv6 system call "event" with the system call arguments and return value.
/// This type is meant to be a "higher-level" representation of a system call as it only contains the arguments which are actually used by a given system call.
/// The arguments are converted to their appropriate types from the "raw" `u64` types used in `SyscallEvent` which represent the underlying 64-bit register values.
#[derive(Clone, Debug)]
pub enum Syscall {
    Fork {
        retval: i32,
    },
    Exit {
        status: i32,
    },
    Wait {
        retval: i32,
        status: u64,
    },
    Pipe {
        retval: i32,
        p: u64,
    }, // TODO: For these (pointers), in the future, it would be good to try to actually display them. Maybe using GDB?
    Read {
        retval: i32,
        fd: i32,
        buf: u64,
        count: i32,
    },
    Kill {
        retval: i32,
        pid: i32,
    },
    Exec {
        retval: i32,
        file: u64,
        argv: u64,
    },
    Fstat {
        retval: i32,
        fd: i32,
        st: u64,
    },
    Chdir {
        retval: i32,
        dir: u64,
    },
    Dup {
        retval: i32,
        fd: i32,
    },
    Getpid {
        retval: i32,
    },
    Sbrk {
        retval: i32,
        n: i32,
    },
    SbrkLazy {
        retval: i32,
        n: i32,
    }, // TODO: Decide whether sbrk and sbrk_lazy should be separate
    Pause {
        retval: i32,
        n: i32,
    },
    Uptime {
        retval: i32,
    },
    Open {
        retval: i32,
        file: u64,
        flags: i32,
    }, // TODO: Properly encode the "flags" enum
    Write {
        retval: i32,
        fd: i32,
        buf: u64,
        count: i32,
    },
    Mknod {
        retval: i32,
        file: u64,
        major: i32,
        minor: i32,
    },
    Unlink {
        retval: i32,
        file: u64,
    },
    Link {
        retval: i32,
        file1: u64,
        file2: u64,
    },
    Mkdir {
        retval: i32,
        dir: u64,
    },
    Close {
        retval: i32,
        fd: i32,
    },
    PStat {
        retval: i32,
        buf: u64,
        size: i32,
    },
    Trace {
        retval: i32,
        pid: i32,
        mask: u32,
    },
    GetTrace {
        retval: i32,
        pid: i32,
        buf: u64,
        size: i32,
    },
}

impl Syscall {
    /// Return description of the system call.
    /// Descriptions are taken from the [xv6 book](https://pdos.csail.mit.edu/6.828/2025/xv6/book-riscv-rev5.pdf).
    pub const fn description(&self) -> &'static str {
        // TODO: Maybe it would be better to do SyscallNum::From
        match self {
            Syscall::Fork { .. } => "Create a process, return child’s PID.",
            Syscall::Exit { .. } => {
                "Terminate the current process; status reported to wait(). No return."
            }
            Syscall::Wait { .. } => {
                "Wait for a child to exit; exit status in *status; returns child PID."
            }
            Syscall::Kill { .. } => "Terminate process PID. Returns 0, or -1 for error.",
            Syscall::Getpid { .. } => "Return the current process’s PID.",
            Syscall::Pause { .. } => "Pause for n clock ticks.",
            Syscall::Exec { .. } => {
                "Load a file and execute it with arguments; only returns if error."
            }
            Syscall::Sbrk { .. } => {
                "Grow process’s memory by n zero bytes. Returns start of new memory."
            }
            Syscall::SbrkLazy { .. } => {
                "Grow process’s memory by n zero bytes. Returns start of new memory."
            } // TODO: Explain the different behind the lazy and eager versions of sbrk
            Syscall::Open { .. } => {
                "Open a file; flags indicate read/write; returns an fd (file descriptor)."
            }
            Syscall::Write { .. } => "Write n bytes from buf to file descriptor fd; returns n.",
            Syscall::Read { .. } => {
                "Read n bytes into buf; returns number read; or 0 if end of file."
            }
            Syscall::Close { .. } => "Release open file fd.",
            Syscall::Dup { .. } => "Return a new file descriptor referring to the same file as fd.",
            Syscall::Pipe { .. } => {
                "Create a pipe, put read/write file descriptors in p[0] and p[1]."
            }
            Syscall::Chdir { .. } => "Change the current directory.",
            Syscall::Mkdir { .. } => "Create a new directory.",
            Syscall::Mknod { .. } => "Create a device file.",
            Syscall::Fstat { .. } => "Place info about an open file into *st.",
            Syscall::Link { .. } => "Create another name (file2) for the file file1.",
            Syscall::Unlink { .. } => "Remove a file",
            Syscall::Uptime { .. } => {
                "Return how many clock tick interrupts have occurred since start."
            } // TODO: Actually check this
            // TODO: Revise these
            Syscall::PStat { .. } => "Place info about the running processes into *buf.",
            Syscall::Trace { .. } => "Set the trace mask for a process.",
            Syscall::GetTrace { .. } => "Put up to n trace events from a process into *buf.",
        }
    }

    /// Returns the "name" of the system call.
    pub fn name(&self) -> &'static str {
        SyscallNum::from(self).name()
    }

    pub fn short_fmt(&self) -> String {
        match self {
            Syscall::Fork { retval } => format!("{}() -> {}", self.name(), retval),
            Syscall::Exit { status } => format!("{}(status: {})", self.name(), status),
            Syscall::Wait { retval, status } => {
                format!("{}(status: {:X}) -> {}", self.name(), status, retval)
            }
            Syscall::Pipe { retval, p } => format!("{}(p: {:X}) -> {}", self.name(), retval, p),
            Syscall::Read {
                retval,
                fd,
                buf,
                count,
            } => format!(
                "{}(fd: {}, buf: {:X}, count: {}) -> {}",
                self.name(),
                fd,
                buf,
                count,
                retval
            ),
            Syscall::Kill { retval, pid } => format!("{}(pid: {}) -> {}", self.name(), pid, retval),
            Syscall::Exec { retval, file, argv } => format!(
                "{}(file: {:X}, argv: {:X}) -> {}",
                self.name(),
                file,
                argv,
                retval
            ),
            Syscall::Fstat { retval, fd, st } => {
                format!("{}(fd: {}, st: {:X}) -> {}", self.name(), fd, st, retval)
            }
            Syscall::Chdir { retval, dir } => {
                format!("{}(dir: {:X}) -> {}", self.name(), dir, retval)
            }
            Syscall::Dup { retval, fd } => format!("{}(fd: {}) -> {}", self.name(), fd, retval),
            Syscall::Getpid { retval } => format!("{}() -> {}", self.name(), retval),
            Syscall::Sbrk { retval, n } => format!("{}(n: {}) -> {}", self.name(), n, retval),
            Syscall::SbrkLazy { retval, n } => format!("{}(n: {}) -> {}", self.name(), n, retval),
            Syscall::Pause { retval, n } => format!("{}(n: {}) -> {}", self.name(), n, retval),
            Syscall::Uptime { retval } => format!("{}() -> {}", self.name(), retval),
            Syscall::Open {
                retval,
                file,
                flags,
            } => format!(
                "{}(file: {:X}, flags: {}) -> {}",
                self.name(),
                file,
                flags,
                retval
            ), // TODO: Print these "in a smart way"
            Syscall::Write {
                retval,
                fd,
                buf,
                count,
            } => format!(
                "{}(fd: {}, buf: {:X}, count: {}) -> {}",
                self.name(),
                fd,
                buf,
                count,
                retval
            ),
            Syscall::Mknod {
                retval,
                file,
                major,
                minor,
            } => format!(
                "{}(file: {:X}, major: {}, minor: {}) -> {}",
                self.name(),
                file,
                major,
                minor,
                retval
            ),
            Syscall::Unlink { retval, file } => {
                format!("{}(file: {:X}) -> {}", self.name(), file, retval)
            }
            Syscall::Link {
                retval,
                file1,
                file2,
            } => format!(
                "{}(file1: {:X}, file2: {:X}) -> {}",
                self.name(),
                file1,
                file2,
                retval
            ),
            Syscall::Mkdir { retval, dir } => {
                format!("{}(dir: {:X}) -> {}", self.name(), dir, retval)
            }
            Syscall::Close { retval, fd } => format!("{}(fd: {}) -> {}", self.name(), fd, retval),
            Syscall::PStat { retval, buf, size } => format!(
                "{}(buf: {:X}, size: {}) -> {}",
                self.name(),
                buf,
                size,
                retval
            ),
            Syscall::Trace { retval, pid, mask } => format!(
                "{}(pid: {}, mask: {:X}) -> {}",
                self.name(),
                pid,
                mask,
                retval
            ),
            Syscall::GetTrace {
                retval,
                pid,
                buf,
                size,
            } => format!(
                "{}(pid: {}, buf: {:X}, size: {}) -> {}",
                self.name(),
                pid,
                buf,
                size,
                retval
            ),
        }
    }
}

// TODO: Maybe switch this to TryFrom due to the possibility of a strange 2nd sbrk argument
impl From<SyscallEvent> for Syscall {
    fn from(event: SyscallEvent) -> Self {
        match event.num {
            SyscallNum::SYS_fork => Syscall::Fork {
                retval: event.retval as i32,
            },
            SyscallNum::SYS_exit => Syscall::Exit {
                status: event.args[0] as i32,
            },
            SyscallNum::SYS_wait => Syscall::Wait {
                retval: event.retval as i32,
                status: event.args[0],
            },
            SyscallNum::SYS_pipe => Syscall::Pipe {
                retval: event.retval as i32,
                p: event.args[0],
            },
            SyscallNum::SYS_read => Syscall::Read {
                retval: event.retval as i32,
                fd: event.args[0] as i32,
                buf: event.args[1],
                count: event.args[2] as i32,
            },
            SyscallNum::SYS_kill => Syscall::Kill {
                retval: event.retval as i32,
                pid: event.args[0] as i32,
            },
            SyscallNum::SYS_exec => Syscall::Exec {
                retval: event.retval as i32,
                file: event.args[0],
                argv: event.args[1],
            },
            SyscallNum::SYS_fstat => Syscall::Fstat {
                retval: event.retval as i32,
                fd: event.args[0] as i32,
                st: event.args[1],
            },
            SyscallNum::SYS_chdir => Syscall::Chdir {
                retval: event.retval as i32,
                dir: event.args[0],
            },
            SyscallNum::SYS_dup => Syscall::Dup {
                retval: event.retval as i32,
                fd: event.args[0] as i32,
            },
            SyscallNum::SYS_getpid => Syscall::Getpid {
                retval: event.retval as i32,
            },
            SyscallNum::SYS_sbrk => {
                if event.args[1] as i32 == SBRK_EAGER {
                    Syscall::Sbrk {
                        retval: event.retval as i32,
                        n: event.args[0] as i32,
                    }
                } else if event.args[1] as i32 == SBRK_LAZY {
                    Syscall::SbrkLazy {
                        retval: event.retval as i32,
                        n: event.args[0] as i32,
                    }
                } else {
                    panic!("Invalid sbrk syscall"); // TODO: Maybe remove the panic path and just log an error
                }
            }
            SyscallNum::SYS_pause => Syscall::Pause {
                retval: event.retval as i32,
                n: event.args[0] as i32,
            },
            SyscallNum::SYS_uptime => Syscall::Uptime {
                retval: event.retval as i32,
            },
            SyscallNum::SYS_open => Syscall::Open {
                retval: event.retval as i32,
                file: event.args[0],
                flags: event.args[1] as i32,
            },
            SyscallNum::SYS_write => Syscall::Write {
                retval: event.retval as i32,
                fd: event.args[0] as i32,
                buf: event.args[1],
                count: event.args[2] as i32,
            },
            SyscallNum::SYS_mknod => Syscall::Mknod {
                retval: event.retval as i32,
                file: event.args[0],
                major: event.args[1] as i32,
                minor: event.args[2] as i32,
            },
            SyscallNum::SYS_unlink => Syscall::Unlink {
                retval: event.retval as i32,
                file: event.args[0],
            },
            SyscallNum::SYS_link => Syscall::Link {
                retval: event.retval as i32,
                file1: event.args[0],
                file2: event.args[1],
            },
            SyscallNum::SYS_mkdir => Syscall::Mkdir {
                retval: event.retval as i32,
                dir: event.args[0],
            },
            SyscallNum::SYS_close => Syscall::Close {
                retval: event.retval as i32,
                fd: event.args[0] as i32,
            },
            SyscallNum::SYS_pstat => Syscall::PStat {
                retval: event.retval as i32,
                buf: event.args[0],
                size: event.args[1] as i32,
            },
            SyscallNum::SYS_trace => Syscall::Trace {
                retval: event.retval as i32,
                pid: event.args[0] as i32,
                mask: event.args[1] as u32,
            },
            SyscallNum::SYS_gettrace => Syscall::GetTrace {
                retval: event.retval as i32,
                pid: event.args[0] as i32,
                buf: event.args[1],
                size: event.args[2] as i32,
            },
        }
    }
}
