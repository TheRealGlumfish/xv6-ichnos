use crate::xv6::memory::MemoryRange;
use crate::xv6::syscall::{self, SyscallNum};
use crate::{Message, TracedProcess};

use iced::border::{self, Border};
use iced::widget::{
    button, center, checkbox, column, container, mouse_area, opaque, row, scrollable, space, stack,
    text, text_input, tooltip,
};
use iced::{Alignment, Background, Color, Element, Length, Theme};
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
        // TODO: Switch to if let here i.e. don't rely on there not being a process with PID -1
        scrollable(column(procs.map(|proc| {
            process_list_element(proc, proc.process.pid == selected_process.unwrap_or(-1))
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
            .text_size(18); // TODO: Investigate increasing text size
        // .size(18);
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
    let syscall_list: Element<'a, Message> = if !process.trace_events.is_empty() {
        // TODO: Allow users to sort either in increasing or decreasing time (currently it shows oldest -> newest)
        let list = column(
            process
                .trace_events
                .iter()
                .map(|event| text(event.short_fmt()).into()),
        )
        .spacing(5);
        scrollable(list).spacing(5).into()
    } else {
        center(text("No traced system calls").style(text::secondary)).into()
    };
    let syscall_list = container(syscall_list)
        .padding(10)
        .height(Length::Fill)
        .width(Length::Fill)
        .align_x(Alignment::Center);
    // .style(|theme| {
    //     container::Style::default()
    //         .background(theme.extended_palette().background.weakest.color)
    // });
    row![sidebar, syscall_list].into()
}

// TODO: On contiguous memory segments it would look nicer to have a ptr in between them.
pub fn memory_segment_view<'a>(segment: MemoryRange) -> Element<'a, Message> {
    container(
        column![
            text!("0x{:010X}", segment.end())
                .size(10)
                .style(text::secondary),
            space().height(2),
            text(segment.name()).size(12), // TODO: Color this?
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
                .spacing(20),
                space::horizontal(),
                column![
                    text("Virtual Kernel Memory"),
                    memory_layout_view(memory.layout_kernel().into_iter().rev())
                ]
                .align_x(Alignment::Center)
                .spacing(20),
                space::horizontal(),
                column![
                    text("Physical Memory"),
                    memory_layout_view(memory.layout_physical().into_iter().rev())
                ]
                .align_x(Alignment::Center)
                .spacing(20),
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
