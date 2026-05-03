use crate::xv6::syscall::{self, SyscallNum};
use crate::{Message, TracedProcess};

use iced::widget::{
    button, center, checkbox, column, container, mouse_area, opaque, row, scrollable, space, stack,
    text, text_input, tooltip,
};
use iced::{Alignment, Background, Color, Element, Length, Theme};
use iced_aw::NumberInput;
use iced_fonts::codicon;
use strum::IntoEnumIterator;

use std::time::Duration;

const TOOLTIP_DELAY: Duration = Duration::from_secs(2);

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

pub fn status_bar<'a>(
    status: impl text::IntoFragment<'a>,
    success: bool,
    last_heartbeat: Option<i32>,
) -> Element<'a, Message> {
    let status_text = container(text(status).size(14)).padding(5);

    let last_heartbeat = if let Some(heartbeat) = last_heartbeat {
        format!("{} ticks", heartbeat)
    } else {
        String::from("N/A")
    };
    let heartbeat_text = text!("Last heartbeat: {}", last_heartbeat).size(14);
    let heartbeat_button = button(heartbeat_text)
        .on_press(Message::RequestHeartbeat)
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

    let status_bar = row![status_text, space::horizontal(), heartbeat_button];

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
        .delay(TOOLTIP_DELAY),
        text("ms"),
        tooltip(
            button(codicon::refresh()).on_press(Message::RequestProcs),
            "Refresh",
            tooltip::Position::Right
        )
        .delay(TOOLTIP_DELAY),
    ]
    .spacing(10)
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
    let button = if process.is_alive {
        button(codicon::close())
            .on_press(Message::KillProcess(proc.pid))
            .style(button::danger)
    } else {
        button(codicon::trash())
            .on_press(Message::DeleteProcess(proc.pid))
            .style(button::danger)
    };
    let elem = row![content, space::horizontal(), button].align_y(Alignment::Center);
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
            .into()
    });
    let checkboxes = scrollable(column(checkboxes).width(Length::Fill));
    let sidebar = column![
        text("Trace Controls")
            .size(16)
            .width(Length::Fill)
            .align_x(Alignment::Center),
        row![
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
        .align_x(Alignment::Center)
        .style(|theme| {
            container::Style::default()
                .background(theme.extended_palette().background.weakest.color)
        });
    row![sidebar, syscall_list].into()
}
