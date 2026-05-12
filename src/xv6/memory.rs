use std::{fmt, io};

use tokio_util::bytes::Buf;

const MEMINFO_SZ: usize = 128;

const PGSIZE: u64 = 4096;
const MAXVA: u64 = 1 << (9 + 9 + 9 + 12 - 1);

const TRAMPOLINE: u64 = MAXVA - PGSIZE;
const TRAPFRAME: u64 = TRAMPOLINE - PGSIZE;
const KERNBASE: u64 = 0x80000000;
const PHYSTOP: u64 = KERNBASE + 128 * 1024 * 1024;

const PLIC: u64 = 0x0c000000;
const UART0: u64 = 0x10000000;
const VIRTIO0: u64 = 0x10001000;
const VIRTIO1: u64 = 0x10002000;

fn pg_round_up(sz: u64) -> u64 {
    (sz + PGSIZE - 1) & !(PGSIZE - 1)
}

fn pg_round_down(a: u64) -> u64 {
    a & !(PGSIZE - 1)
}

#[derive(Clone)]
pub struct MemInfo {
    va_text_end: u64,
    va_data_start: u64,
    va_data_end: u64,
    va_stack_start: u64,
    va_stack_args: u64,
    va_heap_start: u64,
    va_heap_end: u64,
    pa_stack_start: u64,
    pa_stack_end: u64,
    pa_stack_args: u64,
    pa_trapframe_start: u64,
    pa_trampoline_start: u64,
    va_kstack_start: u64,
    pa_kstack_start: u64,
    etext: u64,
    end: u64,
}

impl TryFrom<&[u8]> for MemInfo {
    type Error = io::Error; // TODO: Switch to custom error type

    fn try_from(mut frame: &[u8]) -> io::Result<MemInfo> {
        if frame.len() == MEMINFO_SZ {
            Ok(MemInfo {
                va_text_end: frame.get_u64_le(),
                va_data_start: frame.get_u64_le(),
                va_data_end: frame.get_u64_le(),
                va_stack_start: frame.get_u64_le(),
                va_stack_args: frame.get_u64_le(),
                va_heap_start: frame.get_u64_le(),
                va_heap_end: frame.get_u64_le(),
                pa_stack_start: frame.get_u64_le(),
                pa_stack_args: frame.get_u64_le(),
                pa_stack_end: frame.get_u64_le(),
                pa_trapframe_start: frame.get_u64_le(),
                pa_trampoline_start: frame.get_u64_le(),
                va_kstack_start: frame.get_u64_le(),
                pa_kstack_start: frame.get_u64_le(),
                etext: frame.get_u64_le(),
                end: frame.get_u64_le(),
            })
        } else {
            Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Invalid meminfo frame length {}", frame.len()),
            ))
        }
    }
}

pub enum AddressSpace {
    /// Physical address space, no permissions
    Physical,
    /// Virtual address space, with permissions: read, write, execute, user
    Virtual(bool, bool, bool, bool),
    /// Direct mapped address space (PA=VA), with permissions: read, write, execute, user
    Both(bool, bool, bool, bool),
}

impl fmt::Display for AddressSpace {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AddressSpace::Physical => write!(f, "----"),
            AddressSpace::Virtual(r, w, x, u) => write!(
                f,
                "{}{}{}{}",
                if *r { "R" } else { "-" },
                if *w { "W" } else { "-" },
                if *x { "X" } else { "-" },
                if *u { "U" } else { "-" },
            ),
            AddressSpace::Both(r, w, x, u) => write!(
                f,
                "{}{}{}{}",
                if *r { "R" } else { "-" },
                if *w { "W" } else { "-" },
                if *x { "X" } else { "-" },
                if *u { "U" } else { "-" },
            ),
        }
    }
}

/// Memory segment, end range is exclusive [start, end).
pub struct MemoryRange {
    start: u64,
    end: u64,
    name: &'static str,
    permissions: AddressSpace,
}

impl MemoryRange {
    /// Segment size in bytes.
    pub fn size(&self) -> u64 {
        self.end - self.start
    }

    /// Segment start address.
    pub fn start(&self) -> u64 {
        self.start
    }

    /// Segment end address (inclusive).
    pub fn end(&self) -> u64 {
        self.end - 1
    }

    /// Segment name.
    pub fn name(&self) -> &'static str {
        self.name
    }

    /// Segment permissions.
    pub fn permissions(&self) -> &AddressSpace {
        &self.permissions
    }
}

#[derive(Clone)]
/// Process memory information.
/// Created from an xv6 `meminfo` C struct.
pub struct MemoryInfo {
    meminfo: MemInfo,
}

impl From<MemInfo> for MemoryInfo {
    fn from(meminfo: MemInfo) -> Self {
        MemoryInfo { meminfo }
    }
}

impl MemoryInfo {
    fn text(&self) -> MemoryRange {
        MemoryRange {
            start: 0,
            end: self.meminfo.va_text_end,
            name: "Text",
            permissions: AddressSpace::Virtual(true, false, true, true),
        }
    }

    fn data(&self) -> MemoryRange {
        MemoryRange {
            start: self.meminfo.va_data_start,
            end: self.meminfo.va_data_end,
            name: "Data",
            permissions: AddressSpace::Virtual(true, true, false, true),
        }
    }

    // TODO: Add guard page
    fn guard(&self) -> MemoryRange {
        MemoryRange {
            start: pg_round_up(self.meminfo.va_data_end - 1),
            end: pg_round_down(self.meminfo.va_stack_start), // TODO: Sanity check why we do this (in xv6 too)
            name: "Guard",
            permissions: AddressSpace::Virtual(true, true, false, false),
        }
    }

    fn stack(&self) -> MemoryRange {
        MemoryRange {
            start: self.meminfo.va_stack_start,
            end: self.meminfo.va_heap_start,
            name: "Stack",
            permissions: AddressSpace::Virtual(true, true, false, true),
        }
    }

    // TODO: Maybe make the "stack args" a contiguous part of the stack
    fn stack_args(&self) -> MemoryRange {
        MemoryRange {
            start: self.meminfo.va_stack_args,
            end: self.meminfo.va_heap_start,
            name: "Stack Args",
            permissions: AddressSpace::Virtual(true, true, false, true),
        }
    }

    fn heap(&self) -> MemoryRange {
        MemoryRange {
            start: self.meminfo.va_heap_start,
            end: self.meminfo.va_heap_end,
            name: "Heap",
            permissions: AddressSpace::Virtual(true, true, false, true),
        }
    }

    fn trapframe(&self) -> MemoryRange {
        MemoryRange {
            start: TRAPFRAME,
            end: TRAMPOLINE,
            name: "Trapframe",
            permissions: AddressSpace::Virtual(true, true, false, false),
        }
    }

    fn trampoline(&self) -> MemoryRange {
        MemoryRange {
            start: TRAMPOLINE,
            end: MAXVA,
            name: "Trampoline",
            permissions: AddressSpace::Virtual(true, false, true, false),
        }
    }

    /// User process virtual memory layout (segments are ordered low to high).
    pub fn layout(&self) -> Vec<MemoryRange> {
        vec![
            self.text(),
            self.data(),
            self.guard(),
            self.stack(),
            self.stack_args(),
            self.heap(),
            self.trapframe(),
            self.trampoline(),
        ]
    }

    fn stack_physical(&self) -> MemoryRange {
        MemoryRange {
            start: self.meminfo.pa_stack_start,
            end: self.meminfo.pa_stack_end,
            name: "Stack",
            permissions: AddressSpace::Physical,
        }
    }

    // TODO: Maybe make the "stack args" a contiguous part of the stack
    fn stack_args_physical(&self) -> MemoryRange {
        MemoryRange {
            start: self.meminfo.pa_stack_args,
            end: self.meminfo.pa_stack_end,
            name: "Stack Args",
            permissions: AddressSpace::Physical,
        }
    }

    fn trapframe_physical(&self) -> MemoryRange {
        MemoryRange {
            start: self.meminfo.pa_trapframe_start,
            end: self.meminfo.pa_trapframe_start + PGSIZE,
            name: "Trapframe",
            permissions: AddressSpace::Physical,
        }
    }

    fn trampoline_physical(&self) -> MemoryRange {
        MemoryRange {
            start: self.meminfo.pa_trampoline_start,
            end: self.meminfo.pa_trampoline_start + PGSIZE,
            name: "Trampoline",
            permissions: AddressSpace::Physical,
        }
    }

    fn kernel_stack(&self) -> MemoryRange {
        MemoryRange {
            start: self.meminfo.va_kstack_start,
            end: self.meminfo.va_kstack_start + PGSIZE,
            name: "Kernel Stack",
            permissions: AddressSpace::Virtual(true, true, false, false),
        }
    }

    fn kernel_stack_physical(&self) -> MemoryRange {
        MemoryRange {
            start: self.meminfo.pa_kstack_start,
            end: self.meminfo.pa_kstack_start + PGSIZE,
            name: "Kernel Stack",
            permissions: AddressSpace::Physical,
        }
    }

    fn kernel_plic() -> MemoryRange {
        MemoryRange {
            start: PLIC,
            end: PLIC + 0x4000000,
            name: "PLIC",
            permissions: AddressSpace::Both(true, true, false, false),
        }
    }

    fn kernel_uart0() -> MemoryRange {
        MemoryRange {
            start: UART0,
            end: UART0 + PGSIZE,
            name: "UART0",
            permissions: AddressSpace::Both(true, true, false, false),
        }
    }

    fn kernel_virtio0() -> MemoryRange {
        MemoryRange {
            start: VIRTIO0,
            end: VIRTIO0 + PGSIZE,
            name: "VIRTIO0",
            permissions: AddressSpace::Both(true, true, false, false),
        }
    }

    fn kernel_virtio1() -> MemoryRange {
        MemoryRange {
            start: VIRTIO1,
            end: VIRTIO1 + PGSIZE,
            name: "VIRTIO1",
            permissions: AddressSpace::Both(true, true, false, false),
        }
    }

    fn kernel_text(&self) -> MemoryRange {
        MemoryRange {
            start: KERNBASE,
            end: self.meminfo.etext,
            name: "Kernel Text",
            permissions: AddressSpace::Both(true, false, true, false),
        }
    }

    fn kernel_data(&self) -> MemoryRange {
        MemoryRange {
            start: self.meminfo.etext,
            end: self.meminfo.end,
            name: "Kernel Data",
            permissions: AddressSpace::Both(true, true, false, false),
        }
    }

    // TODO: Somehow indicate free memory (it might clash with the "kernel stack segment")
    /// Kernel virtual memory layout (segments are ordered low to high).
    pub fn layout_kernel(&self) -> Vec<MemoryRange> {
        let mut kernel_ranges = vec![
            MemoryInfo::kernel_plic(),
            MemoryInfo::kernel_uart0(),
            MemoryInfo::kernel_virtio0(),
            MemoryInfo::kernel_virtio1(),
            self.kernel_text(),
            self.trampoline(),
            self.kernel_data(),
            self.kernel_stack(),
        ];
        kernel_ranges.sort_by_key(MemoryRange::start);
        kernel_ranges
    }

    /// Physical memory layout (segments are ordered low to high).
    pub fn layout_physical(&self) -> Vec<MemoryRange> {
        let mut physical_ranges = vec![
            MemoryInfo::kernel_plic(),
            MemoryInfo::kernel_uart0(),
            MemoryInfo::kernel_virtio0(),
            MemoryInfo::kernel_virtio1(),
            self.kernel_text(),
            self.kernel_data(),
            self.trampoline_physical(),
            self.stack_physical(),
            self.stack_args_physical(),
            self.trapframe_physical(),
            self.kernel_stack_physical(),
        ];
        for range in &mut physical_ranges {
            if let AddressSpace::Both(_, _, _, _) = range.permissions {
                range.permissions = AddressSpace::Physical;
            }
        }
        physical_ranges.sort_by_key(MemoryRange::start);
        physical_ranges
    }
    // TODO: Maybe have a "centralized" physical memory view (all kernel stacks, etc.)
}
