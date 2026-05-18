use crate::xv6::memory::{MemoryInfo, MemoryRange, SegmentType};
use crate::xv6::syscall::{self, Syscall, SyscallEvent, SyscallNum};
use crate::{Message, TracedProcess};

use iced::border::{self, Border};
use iced::widget::{
    button, center, checkbox, column, container, mouse_area, opaque, row, scrollable, space, stack,
    text, text_input, tooltip,
};
use iced::{Alignment, Background, Color, Element, Length, Theme, color};
use iced_aw::NumberInput;
use iced_fonts::codicon;
use strum::IntoEnumIterator;

use core::str;
use std::time::Duration;

const TOOLTIP_DELAY: Duration = Duration::from_secs(1);

pub fn modal<'a>(
    base: impl Into<Element<'a, Message>>,
    content: impl Into<Element<'a, Message>>,
    on_blur: Message,
) -> Element<'a, Message> {
    stack![
        base.into(),
        opaque(
            mouse_area(center(opaque(content)).style(|_theme| {
                container::Style {
                    background: Some(
                        Color {
                            a: 0.8,
                            ..Color::BLACK
                        }
                        .into(),
                    ),
                    ..container::Style::default()
                }
            }))
            .on_press(on_blur)
        )
    ]
    .into()
}

pub fn fatal_error_modal<'a>(error: impl text::IntoFragment<'a>) -> Element<'a, Message> {
    container(
        column![
            text("Fatal Error").size(20),
            text(error).style(text::danger),
            button("Exit").on_press(Message::Exit).style(button::danger),
        ]
        .spacing(16)
        .align_x(Alignment::Center),
    )
    .padding(20)
    .style(container::rounded_box)
    .into()
}

pub fn error_modal<'a>(error: impl text::IntoFragment<'a>) -> Element<'a, Message> {
    container(
        column![
            text("Error").size(20),
            text(error).style(text::warning),
            button("Ok")
                .on_press(Message::ClearError)
                .style(button::primary),
        ]
        .spacing(16)
        .align_x(Alignment::Center),
    )
    .padding(20)
    .style(container::rounded_box)
    .into()
}

pub fn exec_modal<'a>(input: &str) -> Element<'a, Message> {
    container(
        column![
            text("Spawn Process").size(20),
            text_input("command", input)
                .on_input(Message::SetExec)
                .on_submit(Message::RequestExec(String::from(input)))
                .width(Length::Fixed(300.0)),
            button("Run") // TODO: Change the name
                .on_press(Message::RequestExec(String::from(input)))
                .style(button::primary),
        ]
        .spacing(16)
        .align_x(Alignment::Center),
    )
    .padding(20)
    .style(container::rounded_box)
    .into()
}

fn status_bar_button<'a>(
    success: bool,
    content: impl Into<Element<'a, Message>>,
    on_press: Message,
    tooltip_text: &'static str,
) -> Element<'a, Message> {
    let button = button(content)
        .on_press(on_press)
        .padding(5)
        .height(Length::Fill)
        .style(move |theme: &Theme, status| {
            let palette = theme.extended_palette();

            match status {
                button::Status::Active | button::Status::Pressed | button::Status::Disabled => {
                    if success {
                        button::Style {
                            background: Some(Background::Color(palette.success.base.color)),
                            text_color: palette.success.base.text,
                            ..button::Style::default()
                        }
                    } else {
                        button::Style {
                            background: Some(Background::Color(palette.danger.base.color)),
                            text_color: palette.danger.base.text,
                            ..button::Style::default()
                        }
                    }
                }
                button::Status::Hovered => {
                    if success {
                        button::Style {
                            background: Some(Background::Color(palette.success.weak.color)),
                            text_color: palette.success.base.text,
                            ..button::Style::default()
                        }
                    } else {
                        button::Style {
                            background: Some(Background::Color(palette.danger.weak.color)),
                            text_color: palette.danger.base.text,
                            ..button::Style::default()
                        }
                    }
                }
            }
        });
    tooltip(button, tooltip_text, tooltip::Position::Top)
        .delay(TOOLTIP_DELAY)
        .style(container::bordered_box)
        .into()
}

pub fn status_bar<'a>(
    status: impl text::IntoFragment<'a>,
    success: bool,
    last_heartbeat: Option<i32>,
) -> Element<'a, Message> {
    let status_text: Element<'_, Message> = if success {
        text(status).size(14).into()
    } else {
        row![codicon::debug_disconnect().size(14), text(status).size(14)]
            .spacing(5)
            .into()
    };
    let status_text = container(status_text).padding(5);

    let last_heartbeat = if let Some(heartbeat) = last_heartbeat {
        format!("{} ticks", heartbeat)
    } else {
        String::from("N/A")
    };
    let heartbeat_text = text!("Last heartbeat: {}", last_heartbeat).size(14);
    let heartbeat_button = status_bar_button(
        success,
        heartbeat_text,
        Message::RequestHeartbeat,
        "Request a heartbeat from xv6",
    );
    let magic_button = status_bar_button(
        success,
        codicon::remote(),
        Message::RequestMagic,
        "Test the connection by sending a magic RPC request",
    );

    let status_bar = row![
        status_text,
        space::horizontal(),
        magic_button,
        heartbeat_button
    ];

    container(status_bar)
        .width(Length::Fill)
        .height(Length::Shrink)
        .style(if success {
            container::success
        } else {
            container::danger
        })
        .into()
}

fn sidebar_header<'a>(proc_period: &u64) -> Element<'a, Message> {
    row![
        tooltip(
            NumberInput::new(proc_period, 0..=10000, Message::ProcRateChange)
                .step(250)
                .width(Length::Fill),
            "Refresh rate (ms)",
            tooltip::Position::Bottom
        )
        .delay(TOOLTIP_DELAY)
        .style(container::bordered_box),
        space().width(10),
        text("ms"),
        space().width(10),
        tooltip(
            button(codicon::refresh()).on_press(Message::RequestProcs),
            "Refresh",
            tooltip::Position::Bottom
        )
        .delay(TOOLTIP_DELAY)
        .style(container::bordered_box),
        tooltip(
            button(codicon::add())
                .on_press(Message::OpenExec)
                .style(button::success),
            "Spawn new process",
            tooltip::Position::Bottom
        )
        .delay(TOOLTIP_DELAY)
        .style(container::bordered_box),
    ]
    .align_y(Alignment::Center)
    .into()
}

// TODO: Add an icon to indicate a process is actively being traced
fn process_list_element<'a>(process: &TracedProcess, selected: bool) -> Element<'a, Message> {
    let proc = &process.process;
    let content = column![
        if proc.ppid == 0 {
            text(format!("{} ({})", proc.name, proc.pid))
        } else {
            text(format!("{} ({}→{})", proc.name, proc.ppid, proc.pid))
        },
        text(format!("{}", proc.state)), // TODO: Maybe color the state
        text(format!("{}", proc.sz)),
    ];
    // TODO: Figure out what to do with zombie processes
    let button = if process.is_alive {
        button(codicon::close())
            .on_press(Message::KillProcess(proc.pid))
            .style(button::danger)
    } else {
        button(codicon::trash())
            .on_press(Message::DeleteProcess(proc.pid))
            .style(button::danger)
    };
    let icon = if process.is_alive {
        if process.is_traced() {
            codicon::debug()
        } else {
            codicon::heart_filled()
        }
    } else {
        codicon::heart() // TODO: Change this to something better
    };
    let elem = row![
        content,
        space::horizontal(),
        icon,
        space().width(Length::Fixed(10.0)),
        button
    ]
    .align_y(Alignment::Center);
    let elem = container(elem)
        .width(Length::Fill)
        .padding(10)
        .style(move |theme: &iced::Theme| {
            let palette = theme.extended_palette();

            let color = if selected {
                palette.secondary.base.color
            } else {
                palette.background.neutral.color
            };
            container::Style::default().background(color)
        });
    mouse_area(elem) // TODO: Maybe make these transparent buttons for the "highlight on hover" effect
        .on_press(Message::SelectProcess(proc.pid))
        .into()
}

pub fn sidebar<'a>(
    proc_period: &u64,
    procs: impl ExactSizeIterator<Item = &'a TracedProcess>,
    selected_process: Option<i32>,
) -> Element<'a, Message> {
    let process_list: Element<'a, Message> = if procs.len() == 0 {
        center(text("No running processes").style(text::secondary)).into()
    } else {
        scrollable(column(procs.map(|proc| {
            process_list_element(proc, Some(proc.process.pid) == selected_process)
        })))
        .into()
    };
    let process_list = container(process_list)
        .height(Length::Fill)
        .width(Length::Fill)
        .style(|theme| {
            container::Style::default()
                .background(theme.extended_palette().background.weakest.color)
        });
    column![sidebar_header(proc_period), process_list].into()
}

pub fn syscall_list_element<'a>(
    syscall: &Syscall,
    pid: i32,
    idx: usize,
    selected: bool,
) -> Element<'a, Message> {
    let elem = button(text(syscall.short_fmt()).size(14))
        .width(Length::Fill)
        .style(move |theme: &Theme, status| {
            let palette = theme.extended_palette();

            if selected {
                button::Style {
                    background: Some(Background::Color(palette.background.strong.color)),
                    ..button::subtle(theme, status)
                }
            } else {
                button::subtle(theme, status)
            }
        })
        .on_press(Message::SelectSyscall(pid, idx));
    tooltip(elem, syscall.description(), tooltip::Position::FollowCursor)
        .delay(TOOLTIP_DELAY)
        .style(container::bordered_box)
        .into()
}

// TODO: Maybe make a "wrapper" of the outside box to use for both this and memory segments
fn trapframe_view<'a>(event: &SyscallEvent) -> Element<'a, Message> {
    let segment = MemoryInfo::trapframe();
    let elem = container(
        column![
            text!("0x{:010X}", segment.end())
                .size(10)
                .style(text::secondary),
            space().height(2),
            text("Trapframe").size(12), // TODO: Color this?
            space().height(2),
            text!("kernel_sp: 0x{:010X}", event.kernel_sp).size(10), // TODO: Left align these
            text("⋮").size(10),
            text!("epc: 0x{:010X}", event.pc).size(10),
            text!("sp: 0x{:010X}", event.sp).size(10), // TODO: Left align these
            text("⋮").size(10),
            text!("a0: 0x{:010X}", event.a0()).size(10),
            text!("a1: 0x{:010X}", event.a1()).size(10),
            text!("a2: 0x{:010X}", event.a2()).size(10),
            text!("a3: 0x{:010X}", event.a3()).size(10),
            text!("a4: 0x{:010X}", event.a4()).size(10),
            text!("a5: 0x{:010X}", event.a5()).size(10),
            text("⋮").size(10),
            text!("a7: 0x{:010X}", event.a7()).size(10),
            space().height(2),
            text(segment.permissions().to_string())
                .size(10)
                .style(text::secondary),
            space().height(2),
            text!("0x{:010X}", segment.start())
                .size(10)
                .style(text::secondary),
        ]
        .width(Length::Fill)
        .align_x(Alignment::Center),
    )
    .style(container::bordered_box)
    .style(|theme| {
        let palette = theme.extended_palette();
        container::Style::default()
            .background(palette.background.base.color)
            .border(Border {
                color: if palette.is_dark {
                    Color::WHITE
                } else {
                    Color::BLACK
                },
                width: 3.0,
                radius: border::radius(0.0),
            })
    })
    .width(Length::Fixed(175.0))
    .padding(5);
    let jump_text = container(
        column![
            text("Context Switch").size(12),
            space().height(2),
            text!("pc: 0x{:010X}→0x{:010X}", event.pc, event.kernel_pc).size(10),
            text!("sp: 0x{:010X}→0x{:010X}", event.sp, event.kernel_sp).size(10),
        ]
        .width(Length::Fill)
        .align_x(Alignment::Center),
    )
    .style(container::bordered_box)
    .style(|theme| {
        let palette = theme.extended_palette();
        container::Style::default()
            .background(palette.background.base.color)
            .border(Border {
                color: if palette.is_dark {
                    Color::WHITE
                } else {
                    Color::BLACK
                },
                width: 3.0,
                radius: border::radius(0.0),
            })
    })
    .width(Length::Fixed(175.0))
    .padding(5);
    let elem = column![elem, jump_text,].spacing(5);
    tooltip(
        elem,
        "Trapframe at the time of system call",
        tooltip::Position::Left,
    ) // TODO: Add text and change to follow cursor
    .delay(TOOLTIP_DELAY)
    .style(container::bordered_box)
    .into()
}

pub fn syscall_view<'a>(process: &'a TracedProcess) -> Element<'a, Message> {
    let checkboxes = SyscallNum::iter().map(|syscall| {
        let checkbox = checkbox(syscall.is_traced(process.trace_mask))
            .label(syscall.name())
            .on_toggle(move |is_checked| {
                let mask = if is_checked {
                    syscall.enable_in_mask(process.trace_mask)
                } else {
                    syscall.disable_in_mask(process.trace_mask)
                };
                Message::ChangeTraceMask(process.process.pid, mask)
            })
            .text_size(18)
            .size(18);
        tooltip(checkbox, syscall.description(), tooltip::Position::Right)
            .delay(TOOLTIP_DELAY)
            .style(container::bordered_box)
            .into()
    });
    let checkboxes = scrollable(column(checkboxes).width(Length::Fill));
    let sidebar = column![
        text("Trace Controls") // TODO: Disable these if not alive
            .size(16)
            .width(Length::Fill)
            .align_x(Alignment::Center),
        row![
            // TODO: Add tooltips
            button("Enable")
                .on_press(Message::ChangeTraceMask(
                    process.process.pid,
                    syscall::ENABLE_ALL_MASK
                ))
                .style(button::success),
            button("Clear")
                .on_press(Message::ChangeTraceMask(
                    process.process.pid,
                    syscall::DISABLE_ALL_MASK
                ))
                .style(button::danger),
            button(codicon::refresh()).on_press(Message::RefreshTrace(process.process.pid)),
        ],
        // TODO: Remove this
        // TypedInput::new("mask", &process.trace_mask)
        //     .on_input(|input| Message::ChangeTraceMask(process.process.pid, input)),
        text_input("mask", format!("0x{:X}", process.trace_mask).as_str()),
        // TODO: Add input parsing and spawn an error if cooked
        // .on_input(|input| Message::ChangeTraceMask(process.process.pid, input)),
        checkboxes
    ]
    .spacing(10);
    let sidebar = container(sidebar)
        .padding(10)
        .width(Length::Shrink)
        .height(Length::Fill);
    let syscall_list: Element<'a, Message> =
        if !process.trace_events.is_empty() {
            // TODO: Allow users to sort either in increasing or decreasing time (currently it shows oldest -> newest)
            let list = column(process.trace_events.iter().enumerate().map(
                |(idx, syscall_event)| {
                    syscall_list_element(
                        &syscall_event.syscall,
                        process.process.pid,
                        idx,
                        process.selected_syscall == Some(idx),
                    )
                },
            ));
            scrollable(list).into()
        } else {
            center(text("No traced system calls").style(text::secondary)).into()
        };
    let syscall_list = container(syscall_list)
        .width(Length::Fill)
        .height(Length::Fill);
    let syscall_pane: Element<'a, Message> = if let Some(selected_idx) = process.selected_syscall {
        let trace_event = &process.trace_events[selected_idx];
        let syscall_body = container(text(trace_event.syscall.manual())).width(Length::Fill);
        row![syscall_body, trapframe_view(trace_event)]
            .spacing(10)
            .into()
    } else {
        center("Select a system call to view details")
            .height(Length::Fixed(300.0))
            .into()
    };
    let main_pane = column![
        syscall_list,
        space().height(20),
        syscall_pane,
        space().height(10.0)
    ];
    row![sidebar, main_pane, space().width(Length::Fixed(10.0))].into() // TODO: Ask someone about the extra "padding" space
}

// TODO: Remove theme and fix color of guard page
fn memory_segment_color(theme: &Theme, segment: &SegmentType) -> Color {
    match segment {
        SegmentType::Text => color!(0x01579B),
        SegmentType::Data => color!(0x0D47A1),
        SegmentType::Guard => theme.extended_palette().background.base.color,
        SegmentType::Stack => color!(0x311B92),
        SegmentType::StackArgs => color!(0x283593),
        SegmentType::Heap => color!(0x4A148C),
        SegmentType::Trapframe => color!(0x880E4F),
        SegmentType::Trampoline => color!(0xB71C1C),
        SegmentType::KernelText => color!(0x3E2723),
        SegmentType::KernelData => color!(0xF57F17),
        SegmentType::KernelStack => color!(0x1B5E20),
        SegmentType::IO(_) => color!(0x263238),
        SegmentType::Other(_) => unimplemented!(),
    }
}

// TODO: On contiguous memory segments it would look nicer to have a ptr in between them.
fn memory_segment_view<'a>(segment: MemoryRange) -> Element<'a, Message> {
    container(
        column![
            text!("0x{:010X}", segment.end())
                .size(10)
                .style(text::secondary),
            space().height(2),
            text(segment.name()).size(12),
            text!("{} B", segment.size()).size(10),
            text(segment.permissions().to_string())
                .size(10)
                .style(text::secondary),
            space().height(2),
            text!("0x{:010X}", segment.start())
                .size(10)
                .style(text::secondary),
        ]
        .width(Length::Fill)
        .align_x(Alignment::Center),
    )
    .width(Length::Fill)
    .style(container::bordered_box)
    .style(move |theme| {
        let palette = theme.extended_palette();
        container::Style::default()
            .background(memory_segment_color(theme, segment.segment_type()))
            .border(Border {
                color: if palette.is_dark {
                    Color::WHITE
                } else {
                    Color::BLACK
                },
                width: 3.0,
                radius: border::radius(0.0),
            })
    })
    .padding(5)
    .into()
}

pub fn memory_layout_view<'a>(segments: impl Iterator<Item = MemoryRange>) -> Element<'a, Message> {
    let segments = column(segments.map(memory_segment_view))
        .width(Length::Fixed(100.0))
        .spacing(5);
    scrollable(segments).spacing(5).into()
}

pub fn memory_view<'a>(process: &'a TracedProcess) -> Element<'a, Message> {
    row![
        button(codicon::refresh()).on_press(Message::RefreshMemory(process.process.pid)),
        if let Some(memory) = &process.memory {
            let elems: Element<'a, Message> = row![
                space::horizontal(),
                column![
                    text("User Virtual Memory"),
                    memory_layout_view(memory.layout().into_iter().rev()),
                ]
                .align_x(Alignment::Center)
                .spacing(15),
                space::horizontal(),
                column![
                    text("Physical Memory"),
                    memory_layout_view(memory.layout_physical().into_iter().rev())
                ]
                .align_x(Alignment::Center)
                .spacing(15),
                space::horizontal(),
                column![
                    text("Virtual Kernel Memory"),
                    memory_layout_view(memory.layout_kernel().into_iter().rev())
                ]
                .align_x(Alignment::Center)
                .spacing(15),
                space::horizontal(),
            ]
            .into();
            elems
        } else {
            center(text("No memory information available, reload")).into()
        }
    ]
    .into()
}
