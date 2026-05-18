use num_enum::TryFromPrimitive;
use strum::{EnumIter, IntoStaticStr};
use tokio_util::bytes::Buf;

use std::{fmt, io};

/// Size of a `syscall_event` C struct in bytes.
pub const SYSCALL_EVENT_SZ: usize = 96;
/// Syscall trace mask with all system calls disabled.
pub const DISABLE_ALL_MASK: u32 = 0;
/// Syscall trace mask with all system calls enabled.
pub const ENABLE_ALL_MASK: u32 = {
    let mut mask = 0;
    let mut i = 1;
    while i <= SyscallNum::SYS_meminfo as u32 {
        mask |= 1 << i;
        i += 1;
    }
    mask
};

const SBRK_EAGER: i32 = 1;
const SBRK_LAZY: i32 = 2;

const O_RDONLY: i32 = 0x000;
const O_WRONLY: i32 = 0x001;
const O_RDWR: i32 = 0x002;
const O_CREATE: i32 = 0x200;
const O_TRUNC: i32 = 0x400;

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
    SYS_meminfo = 25,
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
    pub fn description(&self) -> &'static str {
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
            }
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
            }
            // TODO: Revise these
            SyscallNum::SYS_pstat => "Place info about up to n running processes into *buf.",
            SyscallNum::SYS_trace => "Set the trace mask for a process.",
            SyscallNum::SYS_gettrace => "Put up to n trace events from a process into *buf.",
            SyscallNum::SYS_meminfo => "Put the memory layout of process with PID into *info.",
        }
    }

    /// Returns the the man page of the system call.
    /// Based on the [xv6 book](https://pdos.csail.mit.edu/6.828/2025/xv6/book-riscv-rev5.pdf) and the [Linux man pages](https://www.kernel.org/doc/man-pages/).
    pub fn manual(&self) -> &'static str {
        match self {
            SyscallNum::SYS_fork => {
                "int fork(void);\n\nfork() creates a child process by copying the calling process. The child process is an exact copy of the parent process except for the trace mask (which is not inherited) and the returned value. On success, the PID of the child is returned in the parent, 0 is returned in the child. fork() can fail if the system is out of memory or if the maximum number of processes has been reached."
            }
            SyscallNum::SYS_exit => {
                "int exit(int status) __attribute__((noreturn));\n\nTerminate the current process and return the given exit status to the parent process. This function does not return."
            }
            SyscallNum::SYS_wait => {
                "int wait(int *status);\n\n wait() suspends execution of the calling process until one of its children terminates. On success, the child status is stored in status. On error, -1 is returned. wait() can fail if the calling process has no children."
            }
            SyscallNum::SYS_pipe => {
                "int pipe(int p[2]);\n\npipe() creates a pipe and places the file descriptors into p (p[0] for the read end and p[1] for the write end). On success, 0 is returned. On error, -1 is returned. pipe() can fail if there are no available file descriptors or the system is out of memory."
            }
            SyscallNum::SYS_read => {
                "int read(int fd, void *buf, int n);\n\nread() reads up to n bytes from the file descriptor fd into *buf. On success, the number of bytes read is returned (0 indicates an EOF). On error, -1 is returned. read() can fail if the file descriptor does not support reading."
            }
            SyscallNum::SYS_kill => {
                "int kill(int pid);\n\nkill() requests termination of the process with PID pid. The process is not immediately killed, rather termination is deferred until the process traps into the kernel (e.g. due to a system call or interrupt). Additionally, kill() wakes up the process if sleeping, allowing early termination. On success, 0 is returned. On error, -1 is returned. kill() can fail if there is no process with the given PID."
            }
            SyscallNum::SYS_exec => {
                "int exec(const char *file, const char *argv[]);\n\nexec() replaces the current process with a new process loaded from file and executed with arguments argv (up to MAXARG-1 arguments). On success, exec() does not return. On error, -1 is returned. exec() can fail if the file does not exist or is not a valid executable, or the system is out of memory."
            } // TODO: Make this a little better
            SyscallNum::SYS_fstat => {
                "int fstat(int fd, struct stat *st);\n\nfstat() places information about the file referred to by the file descriptor fd, into *st. On success, 0 is returned. On error, -1 is returned."
            }
            SyscallNum::SYS_chdir => {
                "int chdir(const char *dir);\n\nchdir() changes the current working directory of the calling process to dir.\n\nOn success, 0 is returned. On error, -1 is returned."
            }
            SyscallNum::SYS_dup => {
                "int dup(int fd); dup() allocates a new file descriptor referring to the same file descriptor as fd. On success, the new file descriptor is returned. On error, -1 is returned."
            }
            SyscallNum::SYS_getpid => {
                "int getpid(void);\n\ngetpid() returns the PID of the calling process."
            }
            SyscallNum::SYS_sbrk => {
                "char *sbrk(int n; int t);\nchar *sbrk(int n);\nsbrk_lazy(int n);\n\nsbrk() changes the size of the processes heap segment by n bytes. If n is positive the heap grows, if n is negative the heap shrinks. If t is SBRK_EAGER, the heap is immediately grown or shrunk. If t is SBRK_LAZY, if heap is grown no physical memory is allocated until a page fault occurs on the new heap memory, if the heap is shrunk the physical memory is freed immediately. sbrk(n) and sbrk_lazy(n) are equivalent to sbrk(n, SBRK_EAGER) and sbrk(n, SBRK_LAZY). On success, sbrk() returns the previous end of the heap (i.e. the start of the new memory). On error, SBRK_ERROR (-1) is returned. sbrk() can fail if n is invalid or if the system fails to allocate a page (out of memory)."
            } // TODO: Fix "n is invalid" (actually explain)
            SyscallNum::SYS_pause => {
                "int pause(int n);\n\npause() pauses the calling process for n clock ticks. If n <= 0, pause() returns immediately. On success, 0 is returned. If the process was killed while sleeping, -1 is returned."
            }
            SyscallNum::SYS_uptime => {
                "int uptime(void);\n\nuptime() returns the number of clock ticks since the start of the system."
            }
            SyscallNum::SYS_open => {
                "int open(const char *file, int flags);\n\nopen() opens the file specified by file. If it does not exist and O_CREATE is set in flags, a new file is created. If the file already exists and O_TRUNC is set in flags, the file is truncated to length 0. One of the access modes O_RDONLY (read-only), O_WRONLY (write-only) or O_RDWR (read-write) must be specified in flags, additional bits O_CREATE and O_TRUNC may be or'd. On success, a file descriptor is returned. On error, -1 is returned. open() can fail if the file does not exist and O_CREATE is not set in flags, if the file is a directory and is not opened in mode O_RDONLY, there are not available file descriptors available, or if the system is out of memory."
            }
            SyscallNum::SYS_write => {
                "int write(int fd, const char *buf, int n);\n\nwrite() writes n bytes from buf to the file descriptor fd. On success, the number of bytes written is returned. On error, -1 is returned. write() can fail if the file descriptor does not support writing."
            }
            SyscallNum::SYS_mknod => {
                "int mknod(const char *file, short major, short minor);\n\nmknod() creates a device file at path file with major and minor device numbers. On success, 0 is returned. On error, -1 is returned. mknod() can fail if file is an invalid path or a file already exists at file, or if the device numbers are invalid."
            }
            SyscallNum::SYS_unlink => {
                "int unlink(const char *file);\n\nunlink() deletes the name file from the filesystem. If file is the last link to a file and there are no open file descriptors referring to that file, the file is deleted. On success, 0 is returned. On error, -1 is returned. unlink() can fail if file does not exist or is an invalid path."
            }
            SyscallNum::SYS_link => {
                "int link(const char *file1, const char *file2);\n\nlink() creates a new name (file2) for the file file1. On success, 0 is returned. On error, -1 is returned. link() can fail if file1 does not exist, file2 already exists or is an invalid path, or if file1 is a directory."
            }
            SyscallNum::SYS_mkdir => {
                "int mkdir(const char *dir);\n\nmkdir() creates a new directory at the path dir. On success, 0 is returned. On error, -1 is returned. mkdir() can fail if dir is an invalid path or if a file or directory already exists at dir."
            }
            SyscallNum::SYS_close => {
                "int close(int fd);\n\nclose() closes the file descriptor fd so that it may be reused. On success, 0 is returned. On error, -1 is returned. close() can fail if fd is an invalid file descriptor."
            }
            SyscallNum::SYS_pstat => {
                "int pstat(struct uproc *buf, int n);\n\npstat() is used to obtain information about the currently running processes from the process table. It fills buf with an array of up to n uproc sturcts. On success, the number of processes written to buf is returned. On error, -1 is returned. pstat() can fail if n is negative."
            }
            SyscallNum::SYS_trace => {
                "int trace(int pid, uint mask);\n\ntrace() sets the system call trace mask for the process with PID pid to mask. The trace mask is a bitmask where i-th bit indicates whether the system call with system call number i is traced or not. A mask of 1 enables system call tracing but does not trace any system calls, this can be used to temporarily disable tracing without clearing the system call buffer. A mask of 0 disables all system call tracing and clears the trace buffer. The system call buffer has NTRACE entries. If the trace buffer becomes full the traced process will sleep until free entries become available or tracing is disabled. On success, 0 is returned. On error, -1 is returned. trace() can fail if there is no process with the given PID."
            }
            SyscallNum::SYS_gettrace => {
                "int gettrace(int pid, struct syscall_event *buf, int n);\n\ngettrace() copies and consumes up to n system call trace buffer entries from the process with PID pid into buf. On success, the number of entries copied is returned. On error, -1 is returned. gettrace() can fail if there is no process with the given PID or tracing is disabled for the process."
            }
            SyscallNum::SYS_meminfo => {
                "int meminfo(int pid, struct meminfo *info);\n\nmeminfo() populates *info with the memory layout of the process with PID pid. On success, 0 is returned. On error, -1 is returned. meminfo() can fail if there is no process with the given PID."
            }
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
            Syscall::MemInfo { .. } => SyscallNum::SYS_meminfo,
        }
    }
}

/// A xv6 system call trace buffer entry.
/// Corresponds to a trace buffer entry in xv6, thus it contains the system call (with its arguments and return value), as well as the user and kernel (after the trap into the kernel) program counters and stack pointers at the time of the system call.
#[derive(Clone)]
pub struct SyscallEvent {
    pub syscall: Syscall,
    args: [u64; 6],
    pub pc: u64,
    pub sp: u64,
    pub kernel_pc: u64,
    pub kernel_sp: u64,
}

impl SyscallEvent {
    /// Return the value of a0 register.
    pub fn a0(&self) -> u64 {
        self.args[0]
    }

    /// Return the value of a1 register.
    pub fn a1(&self) -> u64 {
        self.args[1]
    }

    /// Return the value of a2 register.
    pub fn a2(&self) -> u64 {
        self.args[2]
    }

    /// Return the value of a3 register.
    pub fn a3(&self) -> u64 {
        self.args[3]
    }

    /// Return the value of a4 register.
    pub fn a4(&self) -> u64 {
        self.args[4]
    }

    /// Return the value of a5 register.
    pub fn a5(&self) -> u64 {
        self.args[5]
    }

    /// Return the value of a7 register.
    pub fn a7(&self) -> u64 {
        SyscallNum::from(&self.syscall) as u64
    }
}

/// A conversion of an xv6 `syscall_event` C struct in bytes to a `SyscallEventRaw`.
impl TryFrom<&[u8]> for SyscallEvent {
    type Error = io::Error; // TODO: Switch to custom error type

    fn try_from(mut frame: &[u8]) -> io::Result<SyscallEvent> {
        if frame.len() == SYSCALL_EVENT_SZ {
            let num_val = frame.get_i32_le();
            frame.advance(4); // 4 bytes of padding after num
            let args: [u64; 6] = [
                frame.get_u64_le(),
                frame.get_u64_le(),
                frame.get_u64_le(),
                frame.get_u64_le(),
                frame.get_u64_le(),
                frame.get_u64_le(),
            ];
            let retval = frame.get_u64_le();
            Ok(SyscallEvent {
                // num: SyscallNum::try_from(num_val)
                //     .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err.to_string()))?,
                syscall: Syscall::new(
                    SyscallNum::try_from(num_val).map_err(|err| {
                        io::Error::new(io::ErrorKind::InvalidData, err.to_string())
                    })?,
                    args,
                    retval,
                )?,
                args,
                pc: frame.get_u64_le(),
                sp: frame.get_u64_le(),
                kernel_pc: frame.get_u64_le(),
                kernel_sp: frame.get_u64_le(),
            })
        } else {
            Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Invalid syscall event frame length {}", frame.len()),
            ))
        }
    }
}

/// File control flags (fcntl.h) for the open() system call.
/// In xv6, these flags are represented as an integer bitmask and are not strictly mutually exclusive (its not enforced or checked) with the exception of O_RDONLY and O_WRONLY, which cannot be set at the same time.
#[derive(Clone)]
pub struct FileControl {
    pub read: bool,
    pub write: bool,
    read_write: bool,
    pub create: bool,
    pub truncate: bool,
}

/// A conversion of an integer bitmask of the file control flags to `FileControl`.
impl From<i32> for FileControl {
    fn from(flags: i32) -> Self {
        FileControl {
            read: (flags & O_WRONLY) == O_RDONLY,
            write: (flags & O_WRONLY) != 0 || (flags & O_RDWR) != 0,
            read_write: (flags & O_RDWR) != 0,
            create: (flags & O_CREATE) != 0,
            truncate: (flags & O_TRUNC) != 0,
        }
    }
}

impl fmt::Display for FileControl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> std::fmt::Result {
        let mut flags = Vec::new();
        if self.read & !self.write {
            flags.push("O_RDONLY");
        }
        if self.write & !self.read {
            flags.push("O_WRONLY");
        }
        if self.read_write {
            flags.push("O_RDWR");
        }
        if self.create {
            flags.push("O_CREATE");
        }
        if self.truncate {
            flags.push("O_TRUNC");
        }
        write!(f, "{}", flags.join(" | "))
    }
}

/// A xv6 system call with the system call arguments and return value.
/// This type is meant to be a "higher-level" representation of a system call as it only contains the arguments which are actually used by a given system call.
/// The arguments are converted to their appropriate types from the "raw" `u64` types used in `SyscallEvent` which represent the underlying 64-bit register values.
#[derive(Clone)]
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
        flags: FileControl,
    },
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
        n: i32, // TODO: change this in xv6 too
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
        n: i32, // TODO: Change this in xv6 too
    },
    MemInfo {
        retval: i32,
        pid: i32,
        info: u64,
    },
}

impl Syscall {
    pub fn new(num: SyscallNum, args: [u64; 6], retval: u64) -> io::Result<Self> {
        match num {
            SyscallNum::SYS_fork => Ok(Syscall::Fork {
                retval: retval as i32,
            }),
            SyscallNum::SYS_exit => Ok(Syscall::Exit {
                status: args[0] as i32,
            }),
            SyscallNum::SYS_wait => Ok(Syscall::Wait {
                retval: retval as i32,
                status: args[0],
            }),
            SyscallNum::SYS_pipe => Ok(Syscall::Pipe {
                retval: retval as i32,
                p: args[0],
            }),
            SyscallNum::SYS_read => Ok(Syscall::Read {
                retval: retval as i32,
                fd: args[0] as i32,
                buf: args[1],
                count: args[2] as i32,
            }),
            SyscallNum::SYS_kill => Ok(Syscall::Kill {
                retval: retval as i32,
                pid: args[0] as i32,
            }),
            SyscallNum::SYS_exec => Ok(Syscall::Exec {
                retval: retval as i32,
                file: args[0],
                argv: args[1],
            }),
            SyscallNum::SYS_fstat => Ok(Syscall::Fstat {
                retval: retval as i32,
                fd: args[0] as i32,
                st: args[1],
            }),
            SyscallNum::SYS_chdir => Ok(Syscall::Chdir {
                retval: retval as i32,
                dir: args[0],
            }),
            SyscallNum::SYS_dup => Ok(Syscall::Dup {
                retval: retval as i32,
                fd: args[0] as i32,
            }),
            SyscallNum::SYS_getpid => Ok(Syscall::Getpid {
                retval: retval as i32,
            }),
            SyscallNum::SYS_sbrk => {
                if args[1] as i32 == SBRK_EAGER {
                    Ok(Syscall::Sbrk {
                        retval: retval as i32,
                        n: args[0] as i32,
                    })
                } else if args[1] as i32 == SBRK_LAZY {
                    Ok(Syscall::SbrkLazy {
                        retval: retval as i32,
                        n: args[0] as i32,
                    })
                } else {
                    Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!("Invalid sbrk syscall with 2nd argument {}", args[1] as i32),
                    ))
                }
            }
            SyscallNum::SYS_pause => Ok(Syscall::Pause {
                retval: retval as i32,
                n: args[0] as i32,
            }),
            SyscallNum::SYS_uptime => Ok(Syscall::Uptime {
                retval: retval as i32,
            }),
            SyscallNum::SYS_open => Ok(Syscall::Open {
                retval: retval as i32,
                file: args[0],
                flags: FileControl::from(args[1] as i32),
            }),
            SyscallNum::SYS_write => Ok(Syscall::Write {
                retval: retval as i32,
                fd: args[0] as i32,
                buf: args[1],
                count: args[2] as i32,
            }),
            SyscallNum::SYS_mknod => Ok(Syscall::Mknod {
                retval: retval as i32,
                file: args[0],
                major: args[1] as i32,
                minor: args[2] as i32,
            }),
            SyscallNum::SYS_unlink => Ok(Syscall::Unlink {
                retval: retval as i32,
                file: args[0],
            }),
            SyscallNum::SYS_link => Ok(Syscall::Link {
                retval: retval as i32,
                file1: args[0],
                file2: args[1],
            }),
            SyscallNum::SYS_mkdir => Ok(Syscall::Mkdir {
                retval: retval as i32,
                dir: args[0],
            }),
            SyscallNum::SYS_close => Ok(Syscall::Close {
                retval: retval as i32,
                fd: args[0] as i32,
            }),
            SyscallNum::SYS_pstat => Ok(Syscall::PStat {
                retval: retval as i32,
                buf: args[0],
                n: args[1] as i32,
            }),
            SyscallNum::SYS_trace => Ok(Syscall::Trace {
                retval: retval as i32,
                pid: args[0] as i32,
                mask: args[1] as u32,
            }),
            SyscallNum::SYS_gettrace => Ok(Syscall::GetTrace {
                retval: retval as i32,
                pid: args[0] as i32,
                buf: args[1],
                n: args[2] as i32,
            }),
            SyscallNum::SYS_meminfo => Ok(Syscall::MemInfo {
                retval: retval as i32,
                pid: args[0] as i32,
                info: args[1],
            }),
        }
    }

    /// Return description of the system call.
    /// Descriptions are taken from the [xv6 book](https://pdos.csail.mit.edu/6.828/2025/xv6/book-riscv-rev5.pdf).
    pub fn description(&self) -> &'static str {
        SyscallNum::from(self).description()
    }

    /// Returns the the man page of the system call.
    /// Based on the [xv6 book](https://pdos.csail.mit.edu/6.828/2025/xv6/book-riscv-rev5.pdf) and the [Linux man pages](https://www.kernel.org/doc/man-pages/).
    pub fn manual(&self) -> &'static str {
        SyscallNum::from(self).manual()
    }

    /// Returns the "name" of the system call.
    pub fn name(&self) -> &'static str {
        SyscallNum::from(self).name()
    }

    // TODO: Change to a Display trait implementation
    pub fn short_fmt(&self) -> String {
        match self {
            Syscall::Fork { retval } => format!("{}() -> {}", self.name(), retval),
            Syscall::Exit { status } => format!("{}(status: {})", self.name(), status),
            Syscall::Wait { retval, status } => {
                format!("{}(status: 0x{:X}) -> {}", self.name(), status, retval)
            }
            Syscall::Pipe { retval, p } => format!("{}(p: 0x{:X}) -> {}", self.name(), retval, p),
            Syscall::Read {
                retval,
                fd,
                buf,
                count,
            } => format!(
                "{}(fd: {}, buf: 0x{:X}, count: {}) -> {}",
                self.name(),
                fd,
                buf,
                count,
                retval
            ),
            Syscall::Kill { retval, pid } => format!("{}(pid: {}) -> {}", self.name(), pid, retval),
            Syscall::Exec { retval, file, argv } => format!(
                "{}(file: 0x{:X}, argv: 0x{:X}) -> {}",
                self.name(),
                file,
                argv,
                retval
            ),
            Syscall::Fstat { retval, fd, st } => {
                format!("{}(fd: {}, st: 0x{:X}) -> {}", self.name(), fd, st, retval)
            }
            Syscall::Chdir { retval, dir } => {
                format!("{}(dir: 0x{:X}) -> {}", self.name(), dir, retval)
            }
            Syscall::Dup { retval, fd } => format!("{}(fd: {}) -> {}", self.name(), fd, retval),
            Syscall::Getpid { retval } => format!("{}() -> {}", self.name(), retval),
            Syscall::Sbrk { retval, n } => format!("{}(n: {}) -> {}", self.name(), n, retval),
            Syscall::SbrkLazy { retval, n } => {
                format!("{}lazy(n: {}) -> {}", self.name(), n, retval)
            }
            Syscall::Pause { retval, n } => format!("{}(n: {}) -> {}", self.name(), n, retval),
            Syscall::Uptime { retval } => format!("{}() -> {}", self.name(), retval),
            Syscall::Open {
                retval,
                file,
                flags,
            } => format!(
                "{}(file: 0x{:X}, flags: {}) -> {}",
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
                "{}(fd: {}, buf: 0x{:X}, count: {}) -> {}",
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
                "{}(file: 0x{:X}, major: {}, minor: {}) -> {}",
                self.name(),
                file,
                major,
                minor,
                retval
            ),
            Syscall::Unlink { retval, file } => {
                format!("{}(file: 0x{:X}) -> {}", self.name(), file, retval)
            }
            Syscall::Link {
                retval,
                file1,
                file2,
            } => format!(
                "{}(file1: 0x{:X}, file2: 0x{:X}) -> {}",
                self.name(),
                file1,
                file2,
                retval
            ),
            Syscall::Mkdir { retval, dir } => {
                format!("{}(dir: 0x{:X}) -> {}", self.name(), dir, retval)
            }
            Syscall::Close { retval, fd } => format!("{}(fd: {}) -> {}", self.name(), fd, retval),
            Syscall::PStat { retval, buf, n } => {
                format!("{}(buf: 0x{:X}, n: {}) -> {}", self.name(), buf, n, retval)
            }
            Syscall::Trace { retval, pid, mask } => format!(
                "{}(pid: {}, mask: 0x{:X}) -> {}",
                self.name(),
                pid,
                mask,
                retval
            ),
            Syscall::GetTrace {
                retval,
                pid,
                buf,
                n,
            } => format!(
                "{}(pid: {}, buf: 0x{:X}, n: {}) -> {}",
                self.name(),
                pid,
                buf,
                n,
                retval
            ),
            Syscall::MemInfo { retval, pid, info } => format!(
                "{}(pid: {}, info: 0x{:X}) -> {}",
                self.name(),
                pid,
                info,
                retval
            ),
        }
    }
}
